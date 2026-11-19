//! Tiny chiptune sound effects synthesised in code, like `blip/sound.py` (no audio files ship
//! with Blip). Played on a background thread so the pet never waits for audio.

use std::sync::mpsc::{channel, Sender};

const RATE: u32 = 22050;

/// (frequency Hz or 0 for a rest, seconds)
fn tune(name: &str) -> Vec<(f32, f32)> {
    match name {
        "ring" => {
            let mut v = Vec::new();
            for _ in 0..5 {
                v.extend([(1319.0, 0.07), (0.0, 0.02), (1568.0, 0.07), (0.0, 0.02)]);
            }
            v.push((0.0, 0.7));
            v
        }
        "alarm" => {
            let mut v = Vec::new();
            for _ in 0..4 {
                v.extend([(1760.0, 0.11), (0.0, 0.05)]);
            }
            v.push((0.0, 0.45));
            v
        }
        "ding" => vec![(1047.0, 0.07), (1568.0, 0.16)],
        "done" => vec![(784.0, 0.09), (988.0, 0.09), (1175.0, 0.09), (1568.0, 0.22)],
        "pop" => vec![(660.0, 0.04), (990.0, 0.06)],
        "sad" => vec![(523.0, 0.12), (466.0, 0.12), (392.0, 0.25)],
        _ => vec![(523.0, 0.07), (659.0, 0.07), (784.0, 0.07), (1047.0, 0.14)], // sample
    }
}

/// 0..100 -> 0..0.4, on a curve that sounds even to the ear.
pub fn amplitude(volume: f64) -> f32 {
    let v = (volume.clamp(0.0, 100.0) / 100.0) as f32;
    0.4 * v * v
}

pub fn samples(name: &str, amp: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (freq, dur) in tune(name) {
        let n = (RATE as f32 * dur) as usize;
        for i in 0..n {
            let v = if freq <= 0.0 || amp <= 0.0 {
                0.0
            } else {
                let env = 1f32.min(i as f32 / 60.0).min((n - i) as f32 / 200.0); // click-free edges
                let s = (2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32).sin();
                (if s >= 0.0 { 1.0 } else { -1.0 }) * amp * env
            };
            out.push(v);
        }
    }
    out
}

enum Cmd {
    Play(Vec<f32>, bool),
    Stop,
}

pub struct Sound {
    tx: Option<Sender<Cmd>>,
    pub looping: Option<String>,
}

impl Sound {
    pub fn new() -> Sound {
        let (tx, rx) = channel::<Cmd>();
        let ok = std::thread::Builder::new()
            .name("blip-sound".into())
            .spawn(move || {
                use rodio::{OutputStream, Sink, Source};
                let Ok((_stream, handle)) = OutputStream::try_default() else { return };
                let mut sink: Option<Sink> = None;
                while let Ok(cmd) = rx.recv() {
                    if let Some(s) = sink.take() {
                        s.stop();
                    }
                    if let Cmd::Play(data, looped) = cmd {
                        if let Ok(s) = Sink::try_new(&handle) {
                            let src = rodio::buffer::SamplesBuffer::new(1, RATE, data);
                            if looped {
                                s.append(src.buffered().repeat_infinite());
                            } else {
                                s.append(src);
                            }
                            sink = Some(s);
                        }
                    }
                }
            })
            .is_ok();
        Sound { tx: ok.then_some(tx), looping: None }
    }

    /// Play `name` (looped for ringing) at `volume` 0..100; `enabled` false = silent.
    pub fn play(&mut self, name: &str, looped: bool, enabled: bool, volume: f64) {
        if !enabled {
            return;
        }
        if volume <= 0.0 {
            self.stop();
            return;
        }
        self.looping = looped.then(|| name.to_string());
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Play(samples(name, amplitude(volume)), looped));
        }
    }

    /// The sample tune at `volume`, even when sounds are off (for the volume slider).
    pub fn preview(&mut self, volume: f64) {
        self.play("sample", false, true, volume);
    }

    pub fn stop(&mut self) {
        self.looping = None;
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Stop);
        }
    }
}
