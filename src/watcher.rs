use futures_lite::StreamExt;
use mirajazz::{
    device::{DeviceWatcher, list_devices},
    error::MirajazzError,
    types::{DeviceLifecycleEvent, HidDeviceInfo},
};
use openaction::OUTBOUND_EVENT_MANAGER;
use tokio_util::sync::CancellationToken;

use crate::{
    DEVICES, TOKENS, TRACKER,
    device::device_task,
    mappings::{CandidateDevice, DEVICE_NAMESPACE, Kind, QUERIES},
};

fn serial_to_id(serial: &String) -> String {
    format!("{}-{}", DEVICE_NAMESPACE, serial)
}

fn device_info_to_candidate(dev: HidDeviceInfo) -> Option<CandidateDevice> {
    let id = serial_to_id(&dev.serial_number.clone()?);
    let kind = Kind::from_vid_pid(dev.vendor_id, dev.product_id)?;

    log::debug!(
        "Matched candidate id={} kind={} vid=0x{:04x} pid=0x{:04x} serial={}",
        id,
        kind.human_name(),
        dev.vendor_id,
        dev.product_id,
        dev.serial_number
            .clone()
            .unwrap_or_else(|| "<none>".to_string())
    );

    Some(CandidateDevice { id, dev, kind })
}

/// Returns devices that matches known pid/vid pairs
async fn get_candidates() -> Result<Vec<CandidateDevice>, MirajazzError> {
    log::info!("Looking for candidate devices");

    let mut candidates: Vec<CandidateDevice> = Vec::new();

    for dev in list_devices(&QUERIES).await? {
        if let Some(candidate) = device_info_to_candidate(dev.clone()) {
            candidates.push(candidate);
        } else {
            continue;
        }
    }

    Ok(candidates)
}

pub async fn watcher_task(token: CancellationToken) -> Result<(), MirajazzError> {
    let tracker = TRACKER.lock().await.clone();

    // Scans for connected devices that (possibly) we can use
    let candidates = get_candidates().await?;

    log::info!("Looking for connected devices");

    for candidate in candidates {
        log::info!(
            "Found connected candidate id={} kind={} vid=0x{:04x} pid=0x{:04x}",
            candidate.id,
            candidate.kind.human_name(),
            candidate.dev.vendor_id,
            candidate.dev.product_id
        );

        let token = CancellationToken::new();

        TOKENS
            .write()
            .await
            .insert(candidate.id.clone(), token.clone());

        tracker.spawn(device_task(candidate, token));
    }

    let mut watcher = DeviceWatcher::new();
    let mut watcher_stream = watcher.watch(&QUERIES).await?;

    log::info!("Watcher is ready");

    loop {
        let ev = tokio::select! {
            v = watcher_stream.next() => v,
            _ = token.cancelled() => None
        };

        if let Some(ev) = ev {
            log::info!("New device event: {:?}", ev);

            match ev {
                DeviceLifecycleEvent::Connected(info) => {
                    log::info!(
                        "USB connected vid=0x{:04x} pid=0x{:04x} serial={}",
                        info.vendor_id,
                        info.product_id,
                        info.serial_number
                            .clone()
                            .unwrap_or_else(|| "<none>".to_string())
                    );
                    if let Some(candidate) = device_info_to_candidate(info) {
                        // Don't add existing device again
                        if DEVICES.read().await.contains_key(&candidate.id) {
                            log::debug!("Skipping duplicate connected event for {}", candidate.id);
                            continue;
                        }

                        let token = CancellationToken::new();

                        TOKENS
                            .write()
                            .await
                            .insert(candidate.id.clone(), token.clone());

                        log::info!(
                            "Spawning device task for id={} ({})",
                            candidate.id,
                            candidate.kind.human_name()
                        );
                        tracker.spawn(device_task(candidate, token));
                        log::debug!("Spawned");
                    }
                }
                DeviceLifecycleEvent::Disconnected(info) => {
                    log::info!(
                        "USB disconnected vid=0x{:04x} pid=0x{:04x} serial={}",
                        info.vendor_id,
                        info.product_id,
                        info.serial_number
                            .clone()
                            .unwrap_or_else(|| "<none>".to_string())
                    );
                    let Some(serial) = info.serial_number else {
                        log::warn!("Disconnected event without serial, ignoring");
                        continue;
                    };
                    let id = serial_to_id(&serial);

                    if let Some(token) = TOKENS.write().await.remove(&id) {
                        log::info!("Sending cancel request for {}", id);
                        token.cancel();
                    }

                    DEVICES.write().await.remove(&id);

                    if let Some(outbound) = OUTBOUND_EVENT_MANAGER.lock().await.as_mut() {
                        outbound.deregister_device(id.clone()).await.ok();
                    }

                    log::info!("Disconnected device {}", id);
                }
            }
        } else {
            log::info!("Watcher is shutting down");

            break Ok(());
        }
    }
}
