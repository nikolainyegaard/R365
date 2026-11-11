//! PulseAudio/PipeWire device names for the audio pickers. `pactl` is present with both
//! PulseAudio and pipewire-pulse; an unavailable server just yields an empty list.

use std::process::Command;

pub fn sinks() -> Vec<String> {
    names("sinks")
}

pub fn sources() -> Vec<String> {
    names("sources")
}

fn names(kind: &str) -> Vec<String> {
    let out = Command::new("pactl").args(["list", "short", kind]).output();
    match out {
        Ok(out) if out.status.success() => parse(&String::from_utf8_lossy(&out.stdout)),
        _ => Vec::new(),
    }
}

fn parse(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        let _index = parts.next();
        if let Some(name) = parts.next().map(str::trim).filter(|n| !n.is_empty()) {
            names.push(name.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_list_names() {
        let text = "57\talsa_output.usb-Razer.analog-stereo\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED\n58\talsa_input.usb-Razer.mono-fallback\tPipeWire\ts16le 1ch 48000Hz\tSUSPENDED\n";
        assert_eq!(parse(text), ["alsa_output.usb-Razer.analog-stereo", "alsa_input.usb-Razer.mono-fallback"]);
        assert!(parse("").is_empty());
    }
}
