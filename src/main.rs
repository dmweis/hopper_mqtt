use log::*;
use simplelog::*;
use rumqtt::{MqttClient, MqttOptions, QoS, Notification, ReconnectOptions};
use serde::{Serialize, Deserialize};
use rand::Rng;
use std::str;


#[derive(Serialize, Deserialize, Debug)]
struct RelayMessage {
    channel_id: u64,
    content: String
}

#[derive(Serialize, Deserialize, Debug)]
enum MessageWrapper {
    Message(RelayMessage),
    KeepAlive
}

fn send_to_discord(client: &mut MqttClient, message: &str) {
    info!("Sending message to discord");
    let hopper_channel_id = 699300787746111528_u64;
    let message = MessageWrapper::Message(RelayMessage {
        channel_id: hopper_channel_id,
        content: message.to_owned(),
    });
    if let Ok(payload) = serde_json::to_string(&message) {
        if let Err(error) = client.publish("discord/send", QoS::AtLeastOnce, false, payload) {
            warn!("failed to send MQTT message {}", error);
        }
    } else {
        warn!("failed to serialize message");
    }
}

fn main() {
    // println!("{}", env!("CARGO_PKG_NAME"));
    let config = ConfigBuilder::new()
        .add_filter_allow_str(env!("CARGO_PKG_NAME"))
        .build();
    if TermLogger::init(LevelFilter::Info, config.clone(), TerminalMode::Mixed).is_err() {
        eprintln!("Failed to create term logger");
        if SimpleLogger::init(LevelFilter::Info, config).is_err() {
            eprintln!("Failed to create simple logger");
        }
    }
    let mut rng = rand::thread_rng();
    let hopper_channel_id = 699300787746111528_u64;

    // topics
    let voltage_topic = "hopper/telemetry/voltage";
    let warning_voltage_topic = "hopper/telemetry/warning_voltage";
    let incoming_discord_message_topic = &format!("discord/receive/{}", hopper_channel_id);

    let topics = vec![
        voltage_topic,
        warning_voltage_topic,
        incoming_discord_message_topic,
    ];

    // mutable data
    let mut warning_sent = false;
    let mut warning_voltage = 10.5;
    let mut last_voltage = 0.0;

    let mqtt_options = MqttOptions::new(format!("mqtt_hopper_server{}", rng.gen::<u64>()), "mqtt.local", 1883)
        .set_reconnect_opts(ReconnectOptions::Always(5));
    let (mut mqtt_client, notifications) = MqttClient::start(mqtt_options)
                                              .expect("Failed to connect to MQTT host");
    info!("Connected to MQTT");

    for topic_name in &topics {
        mqtt_client.subscribe(topic_name.to_owned(), QoS::AtMostOnce)
                   .expect(&format!("Failed to subscribe to topic {}", topic_name));
        trace!("Subscribing to {}", topic_name);
    }

    for notification in notifications {
        if let Notification::Publish(data) = notification {
            trace!("New message");
            match data.topic_name.as_str() {
                "hopper/telemetry/voltage" => {
                    trace!("voltage_topic");
                    if let Ok(message_text) = str::from_utf8(&data.payload) {
                        if let Ok(voltage) = message_text.parse::<f32>() {
                            last_voltage = voltage;
                            if voltage < warning_voltage && !warning_sent {
                                warning_sent = true;
                                send_to_discord(&mut mqtt_client, &format!("Voltage low: {}", voltage));
                            }
                            if voltage > warning_voltage {
                                if warning_sent {
                                    send_to_discord(&mut mqtt_client, &format!("Voltage good: {}", voltage));
                                }
                                warning_sent = false;
                            }
                        }
                    } else {
                        error!("Failed to parse MQTT payload");
                    }
                },
                "hopper/telemetry/warning_voltage" => {
                    trace!("warning_voltage_topic");
                    if let Ok(message_text) = str::from_utf8(&data.payload) {
                        if let Ok(voltage) = message_text.parse::<f32>() {
                            warning_voltage = voltage;
                            info!("Set new warning voltage of {}", voltage);
                        }
                    }
                },
                "discord/receive/699300787746111528" => {
                    info!("incoming_discord_message_topic");
                    if let Ok(text) = str::from_utf8(&data.payload) {
                        if text.to_ascii_lowercase().trim() == "voltage" {
                            send_to_discord(&mut mqtt_client, &format!("Current voltage is {}", last_voltage))
                        }
                    } else {
                        warn!("failed to parse incoming discord message");
                    }
                }
                _ => {
                    warn!("Unknown topic")
                }
            }
        } else if let Notification::Disconnection = notification {
            warn!("Client disconnected from MQTT");
        } else if let Notification::Reconnection = notification {
            for topic_name in &topics {
                mqtt_client.subscribe(topic_name.to_owned(), QoS::AtMostOnce)
                           .expect(&format!("Failed to subscribe to topic {}", topic_name));
            }
            warn!("Client reconnected to MQTT");
        }
    }
}
