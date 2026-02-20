use mirajazz::{error::MirajazzError, types::DeviceInput};
use std::sync::Mutex;

const KEY_COUNT_N1: usize = 17;
const ENCODER_COUNT_N1: usize = 1;

static ENCODER_STATES: Mutex<[bool; ENCODER_COUNT_N1]> = Mutex::new([false; ENCODER_COUNT_N1]);

pub fn process_input_n1(input: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    log::debug!("Processing input (N1): {input}=0x{input:02x}=0b{input:08b}, {state}");

    // N1 periodically emits this non-input status frame.
    if input == 0xcc && state == 0xff {
        log::debug!("Ignoring N1 status frame: code=0xcc state=0xff");
        return Ok(DeviceInput::NoData);
    }

    let decoded = match input {
        (0x01..=0x0f) | 0x1e | 0x1f => read_button_press_n1(input, state),
        0x32 | 0x33 => read_encoder_value_n1(input),
        0x23 => read_encoder_press_n1(state),
        _ => Err(MirajazzError::BadData),
    };

    if decoded.is_err() {
        log::debug!("Ignoring unknown input (N1 parser): code=0x{input:02x} state=0x{state:02x}");
        return Ok(DeviceInput::NoData);
    }

    decoded
}

fn read_button_states(states: &[u8], key_count: usize) -> Vec<bool> {
    let mut bools = vec![];

    for i in 0..key_count {
        bools.push(states[i + 1] != 0);
    }

    bools
}

fn read_button_press_n1(input: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    let mut button_states = vec![0x01];
    button_states.extend(vec![0u8; KEY_COUNT_N1 + 1]);

    let pressed_index: usize = match input {
        0x01..=0x0f => input as usize, // Display keys 1..15
        0x1e => 16,                    // Top button left
        0x1f => 17,                    // Top button right
        _ => return Err(MirajazzError::BadData),
    };

    button_states[pressed_index] = state;
    log::debug!(
        "Decoded N1 button raw=0x{input:02x} -> logical={} state={}",
        pressed_index,
        state
    );

    Ok(DeviceInput::ButtonStateChange(read_button_states(
        &button_states,
        KEY_COUNT_N1,
    )))
}

fn read_encoder_value_n1(input: u8) -> Result<DeviceInput, MirajazzError> {
    let mut encoder_values = vec![0i8; ENCODER_COUNT_N1];

    let value: i8 = match input {
        0x32 => -1,
        0x33 => 1,
        _ => return Err(MirajazzError::BadData),
    };

    encoder_values[0] = value;
    log::debug!(
        "Decoded N1 encoder twist raw=0x{input:02x} -> encoder=0 delta={}",
        value
    );
    Ok(DeviceInput::EncoderTwist(encoder_values))
}

fn read_encoder_press_n1(state: u8) -> Result<DeviceInput, MirajazzError> {
    let mut states = ENCODER_STATES.lock().unwrap();
    states[0] = state == 0x01;
    let encoder_states = states.to_vec();
    drop(states);

    log::debug!("N1 encoder states: {:#?}", encoder_states);
    Ok(DeviceInput::EncoderStateChange(encoder_states))
}
