use data_url::DataUrl;
use image::load_from_memory_with_format;
use mirajazz::{device::Device, error::MirajazzError, state::DeviceStateUpdate};
use openaction::{OUTBOUND_EVENT_MANAGER, SetImageEvent};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;

use crate::{
    DEVICES, TOKENS,
    mappings::{CandidateDevice, Kind},
};

const N1_UI_POS_TOP_LEFT: u8 = 0;
const N1_UI_POS_TOP_MIDDLE: u8 = 1;
const N1_UI_POS_TOP_RIGHT_ENCODER: u8 = 2;
const N1_UI_POS_LCD_LEFT: u8 = 3;
const N1_UI_POS_LCD_MIDDLE: u8 = 4;
const N1_UI_POS_LCD_RIGHT: u8 = 5;
const N1_UI_GRID_START: u8 = 6;
const N1_UI_GRID_END: u8 = 20;

const N1_LOGICAL_TOP_LEFT: u8 = 15;
const N1_LOGICAL_TOP_RIGHT: u8 = 16;

const N1_HW_KEY_START: u8 = 0;
const N1_HW_KEY_END: u8 = 14;
const N1_HW_SEGMENT_START: u8 = 15;
const N1_HW_SEGMENT_END: u8 = 17;

static N1_MAPPING_LOGGED: AtomicBool = AtomicBool::new(false);

/// Initializes a device and listens for events
pub async fn device_task(candidate: CandidateDevice, token: CancellationToken) {
    log::info!(
        "Running device task id={} kind={} vid=0x{:04x} pid=0x{:04x}",
        candidate.id,
        candidate.kind.human_name(),
        candidate.dev.vendor_id,
        candidate.dev.product_id
    );

    // Wrap in a closure so we can use `?` operator
    let device = async || -> Result<Device, MirajazzError> {
        let device = connect(&candidate).await?;

        if matches!(candidate.kind, Kind::VsdInsideN1) {
            let mode = env::var("OPENDECK_AKP05_N1_MODE")
                .ok()
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(3);
            log::info!(
                "Setting device {} ({}) to startup mode {}",
                candidate.id,
                candidate.kind.human_name(),
                mode
            );
            device.set_mode(mode).await?;
        }

        device.set_brightness(50).await?;
        device.clear_all_button_images().await?;
        device.flush().await?;

        Ok(device)
    }()
    .await;

    let device: Device = match device {
        Ok(device) => device,
        Err(err) => {
            handle_error(&candidate.id, err).await;

            log::error!(
                "Had error during device init, finishing device task: {:?}",
                candidate
            );

            return;
        }
    };

    log::info!("Registering device {}", candidate.id);
    if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
        log::debug!(
            "register_device id={} name={} rows={} cols={} encoders={} type={}",
            candidate.id,
            candidate.kind.human_name(),
            candidate.kind.row_count(),
            candidate.kind.col_count(),
            candidate.kind.encoder_count(),
            candidate.kind.device_type()
        );
        outbound
            .register_device(
                candidate.id.clone(),
                candidate.kind.human_name(),
                candidate.kind.row_count() as u8,
                candidate.kind.col_count() as u8,
                candidate.kind.encoder_count() as u8,
                candidate.kind.device_type(),
            )
            .await
            .unwrap();
    }

    DEVICES.write().await.insert(candidate.id.clone(), device);

    tokio::select! {
        _ = device_events_task(&candidate) => {},
        _ = keepalive_task(&candidate) => {},
        _ = token.cancelled() => {}
    };

    log::info!("Shutting down device {:?}", candidate);

    if let Some(device) = DEVICES.read().await.get(&candidate.id) {
        device.shutdown().await.ok();
    }

    log::info!("Device task finished for {:?}", candidate);
}

/// Sends periodic keepalive packets to reduce idle-time disconnects on some devices.
async fn keepalive_task(candidate: &CandidateDevice) -> Result<(), MirajazzError> {
    const KEEPALIVE_INTERVAL_SECS: u64 = 10;

    loop {
        sleep(Duration::from_secs(KEEPALIVE_INTERVAL_SECS)).await;

        let devices = DEVICES.read().await;
        let Some(device) = devices.get(&candidate.id) else {
            log::debug!(
                "Keepalive task stopped, device {} is not in map",
                candidate.id
            );
            return Ok(());
        };

        if let Err(e) = device.keep_alive().await {
            drop(devices);
            log::warn!("Keepalive packet failed for {}: {}", candidate.id, e);
            if !handle_error(&candidate.id, e).await {
                return Ok(());
            }
        } else {
            log::debug!("Keepalive packet sent for {}", candidate.id);
        }
    }
}

/// Handles errors, returning true if should continue, returning false if an error is fatal
pub async fn handle_error(id: &String, err: MirajazzError) -> bool {
    log::error!("Device {} error: {}", id, err);

    // Some errors are not critical and can be ignored without sending disconnected event
    if matches!(err, MirajazzError::ImageError(_) | MirajazzError::BadData) {
        return true;
    }

    log::info!("Deregistering device {}", id);
    if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
        outbound.deregister_device(id.clone()).await.unwrap();
    }

    log::info!("Cancelling tasks for device {}", id);
    if let Some(token) = TOKENS.read().await.get(id) {
        token.cancel();
    }

    log::info!("Removing device {} from the list", id);
    DEVICES.write().await.remove(id);

    log::info!("Finished clean-up for {}", id);

    false
}

pub async fn connect(candidate: &CandidateDevice) -> Result<Device, MirajazzError> {
    const MAX_CONNECT_ATTEMPTS: u8 = 10;
    const RETRY_DELAY_MS: u64 = 300;

    log::debug!(
        "Connecting id={} protocol={} keys={} encoders={}",
        candidate.id,
        candidate.kind.protocol_version(),
        candidate.kind.key_count(),
        candidate.kind.encoder_count()
    );
    for attempt in 1..=MAX_CONNECT_ATTEMPTS {
        let result = Device::connect(
            &candidate.dev,
            candidate.kind.protocol_version(),
            candidate.kind.key_count(),
            candidate.kind.encoder_count(),
        )
        .await;

        match result {
            Ok(device) => {
                log::info!(
                    "Connected id={} (runtime vid=0x{:04x} pid=0x{:04x}) after attempt {}/{}",
                    candidate.id,
                    device.vid,
                    device.pid,
                    attempt,
                    MAX_CONNECT_ATTEMPTS
                );
                return Ok(device);
            }
            Err(e) => {
                let msg = e.to_string();
                let retryable = msg.contains("Permission denied")
                    || msg.contains("Resource busy")
                    || msg.contains("Disconnected");

                if retryable && attempt < MAX_CONNECT_ATTEMPTS {
                    log::warn!(
                        "Connect attempt {}/{} failed for {}: {}. Retrying in {}ms",
                        attempt,
                        MAX_CONNECT_ATTEMPTS,
                        candidate.id,
                        msg,
                        RETRY_DELAY_MS
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    continue;
                }

                log::error!("Error while connecting to device: {e}");
                return Err(e);
            }
        }
    }

    unreachable!("connect loop always returns");
}

/// Handles events from device to OpenDeck
async fn device_events_task(candidate: &CandidateDevice) -> Result<(), MirajazzError> {
    log::info!("Connecting to {} for incoming events", candidate.id);

    let devices_lock = DEVICES.read().await;
    let reader = match devices_lock.get(&candidate.id) {
        Some(device) => device.get_reader(crate::inputs::process_input_n1),
        None => return Ok(()),
    };
    drop(devices_lock);

    log::info!("Connected to {} for incoming events", candidate.id);

    log::info!("Reader is ready for {}", candidate.id);

    loop {
        log::debug!("Reading updates...");

        let updates = match reader.read(None).await {
            Ok(updates) => updates,
            Err(e) => {
                if !handle_error(&candidate.id, e).await {
                    break;
                }

                continue;
            }
        };

        for update in updates {
            log_n1_mapping_once();
            log::debug!("New update: {:#?}", update);

            let id = candidate.id.clone();

            if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
                match update {
                    DeviceStateUpdate::ButtonDown(key) => {
                        match map_input_key_to_ui(key) {
                            Some(mapped) => {
                                log::info!(
                                    "EVENT device={} ButtonDown key={} mapped_key={}",
                                    id,
                                    key,
                                    mapped
                                );
                                outbound.key_down(id, mapped).await.unwrap();
                            }
                            None => {
                                log::debug!(
                                    "Ignoring unmapped input key={} for {}",
                                    key,
                                    candidate.kind.human_name()
                                );
                            }
                        }
                    }
                    DeviceStateUpdate::ButtonUp(key) => {
                        match map_input_key_to_ui(key) {
                            Some(mapped) => {
                                log::info!(
                                    "EVENT device={} ButtonUp key={} mapped_key={}",
                                    id,
                                    key,
                                    mapped
                                );
                                outbound.key_up(id, mapped).await.unwrap();
                            }
                            None => {
                                log::debug!(
                                    "Ignoring unmapped input key={} for {}",
                                    key,
                                    candidate.kind.human_name()
                                );
                            }
                        }
                    }
                    DeviceStateUpdate::EncoderDown(encoder) => {
                        log::info!("EVENT device={} EncoderDown encoder={}", id, encoder);
                        outbound.encoder_down(id, encoder).await.unwrap();
                    }
                    DeviceStateUpdate::EncoderUp(encoder) => {
                        log::info!("EVENT device={} EncoderUp encoder={}", id, encoder);
                        outbound.encoder_up(id, encoder).await.unwrap();
                    }
                    DeviceStateUpdate::EncoderTwist(encoder, val) => {
                        log::info!(
                            "EVENT device={} EncoderTwist encoder={} delta={}",
                            id,
                            encoder,
                            val
                        );
                        outbound
                            .encoder_change(id, encoder, val as i16)
                            .await
                            .unwrap();
                    }
                }
            }
        }
    }

    Ok(())
}

fn log_n1_mapping_once() {
    if N1_MAPPING_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }

    log::info!("N1 mapping self-test (one-time)");
    log::info!(
        "N1 UI top row: top-left key={}, top-middle key={}, top-right encoder={}",
        N1_UI_POS_TOP_LEFT,
        N1_UI_POS_TOP_MIDDLE,
        N1_UI_POS_TOP_RIGHT_ENCODER
    );
    log::info!(
        "N1 UI LCD row (button targets): left={}, middle={}, right={}",
        N1_UI_POS_LCD_LEFT,
        N1_UI_POS_LCD_MIDDLE,
        N1_UI_POS_LCD_RIGHT
    );
    log::info!(
        "N1 input mapping: hw_top_left={} -> ui_key={}, hw_top_middle={} -> ui_key={}",
        N1_LOGICAL_TOP_LEFT,
        N1_UI_POS_TOP_LEFT,
        N1_LOGICAL_TOP_RIGHT,
        N1_UI_POS_TOP_MIDDLE
    );
    log::info!(
        "N1 top buttons are input-only: logical keys {} and {} do not support images",
        N1_LOGICAL_TOP_LEFT,
        N1_LOGICAL_TOP_RIGHT
    );
    log::info!(
        "N1 input mapping: hw_keypad {}..{} -> ui_key {}..{} (offset +{})",
        N1_HW_KEY_START,
        N1_HW_KEY_END,
        N1_UI_GRID_START,
        N1_UI_GRID_END,
        N1_UI_GRID_START
    );
    log::info!(
        "N1 image mapping (Keypad): ui_key {}..{} -> hw_button {}..{}",
        N1_UI_GRID_START,
        N1_UI_GRID_END,
        N1_HW_KEY_START,
        N1_HW_KEY_END
    );
    log::info!(
        "N1 image mapping (LCD row): ui_key {}..{} -> hw_button {}..{}",
        N1_UI_POS_LCD_LEFT,
        N1_UI_POS_LCD_RIGHT,
        N1_HW_SEGMENT_START,
        N1_HW_SEGMENT_END
    );
}

fn map_input_key_to_ui(key: u8) -> Option<u8> {
    match key {
        0..=14 => Some(key + N1_UI_GRID_START),
        N1_LOGICAL_TOP_LEFT => Some(N1_UI_POS_TOP_LEFT),
        N1_LOGICAL_TOP_RIGHT => Some(N1_UI_POS_TOP_MIDDLE),
        _ => None,
    }
}

fn map_key_image_position_to_hw(position: u8) -> Result<Option<u8>, MirajazzError> {
    let mapped = match position {
        // Top row: two input-only keys + encoder spot.
        N1_UI_POS_TOP_LEFT | N1_UI_POS_TOP_MIDDLE | N1_UI_POS_TOP_RIGHT_ENCODER => return Ok(None),
        // LCD row: left/middle/right segment targets.
        N1_UI_POS_LCD_LEFT => return Ok(Some(15)),
        N1_UI_POS_LCD_MIDDLE => return Ok(Some(16)),
        N1_UI_POS_LCD_RIGHT => return Ok(Some(17)),
        // Keypad rows in UI (positions 6..20) map to hardware image keys 0..14.
        N1_UI_GRID_START..=N1_UI_GRID_END => position - N1_UI_GRID_START,
        _ => return Err(MirajazzError::BadData),
    };

    if (N1_HW_KEY_START..=N1_HW_KEY_END).contains(&mapped) {
        Ok(Some(mapped))
    } else {
        Err(MirajazzError::BadData)
    }
}

/// Handles different combinations of "set image" event, including clearing the specific buttons and whole device
pub async fn handle_set_image(device: &Device, evt: SetImageEvent) -> Result<(), MirajazzError> {
    let is_encoder = evt.controller.as_deref() == Some("Encoder");
    let kind = Kind::VsdInsideN1;

    log::debug!(
        "SetImage request device(vid=0x{:04x},pid=0x{:04x},kind={}) position={:?} controller={:?} has_image={}",
        device.vid,
        device.pid,
        kind.human_name(),
        evt.position,
        evt.controller,
        evt.image.is_some()
    );

    match (evt.position, evt.image) {
        (Some(position), Some(image)) => {
            if is_encoder {
                log::debug!(
                    "Ignoring encoder image set at position={} (encoder is input-only on N1)",
                    position
                );
                return Ok(());
            }

            log::debug!("Setting image for requested position {}", position);
            let mapped = map_key_image_position_to_hw(position)?.map(|v| vec![v]);

            let Some(positions) = mapped else {
                log::debug!(
                    "Ignoring image set for input-only/unused UI position={} (controller={:?})",
                    position,
                    evt.controller
                );
                return Ok(());
            };
            log::debug!("Mapped image positions={:?} (is_encoder={}) for kind={}", positions, is_encoder, kind.human_name());

            // OpenDeck sends image as a data url, so parse it using a library
            let url = DataUrl::process(image.as_str()).unwrap(); // Isn't expected to fail, so unwrap it is
            let (body, _fragment) = url.decode_to_vec().unwrap(); // Same here

            // Allow only image/jpeg mime for now
            if url.mime_type().subtype != "jpeg" {
                log::error!("Incorrect mime type: {}", url.mime_type());

                return Ok(()); // Not a fatal error, enough to just log it
            }

            let image = load_from_memory_with_format(body.as_slice(), image::ImageFormat::Jpeg)?;

            for hw_pos in positions {
                let image_format = if (N1_HW_SEGMENT_START..=N1_HW_SEGMENT_END).contains(&hw_pos) {
                    kind.touch_image_format()
                } else {
                    kind.image_format()
                };
                device
                    .set_button_image(hw_pos, image_format, image.clone())
                    .await?;
            }
            device.flush().await?;
        }
        (Some(position), None) => {
            if is_encoder {
                log::debug!(
                    "Ignoring encoder image clear at position={} (encoder is input-only on N1)",
                    position
                );
                return Ok(());
            }

            let mapped = map_key_image_position_to_hw(position)?.map(|v| vec![v]);

            let Some(positions) = mapped else {
                log::debug!(
                    "Ignoring clear for input-only/unused UI position={} (controller={:?})",
                    position,
                    evt.controller
                );
                return Ok(());
            };
            log::debug!("Clearing image at mapped positions={:?} (is_encoder={})", positions, is_encoder);
            for hw_pos in positions {
                device.clear_button_image(hw_pos).await?;
            }
            device.flush().await?;
        }
        (None, None) => {
            log::debug!("Clearing all button images");
            device.clear_all_button_images().await?;
            device.flush().await?;
        }
        _ => {}
    }

    Ok(())
}
