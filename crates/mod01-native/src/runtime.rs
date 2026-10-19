use std::{
    cell::Cell,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use serde_json::Value;

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

struct HostProcess {
    child: Child,
    replies: Receiver<Result<Value, String>>,
}

impl HostProcess {
    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct RuntimeBridge {
    sender: Sender<(u64, Value)>,
    receiver: Receiver<(Option<u64>, Value)>,
    generation: Arc<AtomicU64>,
    pending: Cell<usize>,
}

impl RuntimeBridge {
    pub fn new(host_path: PathBuf) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let active = Arc::clone(&generation);
        thread::spawn(move || worker(host_path, command_rx, event_tx, active));
        Self {
            sender: command_tx,
            receiver: event_rx,
            generation,
            pending: Cell::new(0),
        }
    }

    pub fn send(&self, command: Value) -> Result<(), String> {
        let replaces_session = matches!(
            command["type"].as_str(),
            Some("load" | "render-wav" | "audio-preview-load" | "stop")
        );
        let generation = if replaces_session {
            self.generation
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1)
        } else {
            self.generation.load(Ordering::Relaxed)
        };
        self.sender
            .send((generation, command))
            .map_err(|error| error.to_string())?;
        self.pending.set(if replaces_session {
            1
        } else {
            self.pending.get().saturating_add(1)
        });
        Ok(())
    }

    pub fn has_pending(&self) -> bool {
        self.pending.get() != 0
    }

    pub fn poll(&self) -> Option<Value> {
        while let Ok((generation, event)) = self.receiver.try_recv() {
            if generation
                .is_none_or(|generation| generation == self.generation.load(Ordering::Relaxed))
            {
                self.pending.set(if event["type"] == "error" {
                    0
                } else {
                    self.pending.get().saturating_sub(1)
                });
                return Some(event);
            }
        }
        None
    }
}

fn worker(
    host_path: PathBuf,
    commands: Receiver<(u64, Value)>,
    events: Sender<(Option<u64>, Value)>,
    active: Arc<AtomicU64>,
) {
    let host_path = match host_path.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            let _ = events.send((
                None,
                error_event(format!("native runtime bundle is missing: {error}")),
            ));
            return;
        }
    };
    let mut child: Option<HostProcess> = None;
    for (generation, command) in commands {
        if generation != active.load(Ordering::Relaxed) {
            continue;
        }
        let publish = |event| events.send((Some(generation), event));
        if command["type"] == "load"
            || command["type"] == "render-wav"
            || command["type"] == "audio-preview-load"
        {
            if let Some(old) = child.take() {
                old.stop();
            }
            let bundled_node = host_path
                .parent()
                .map(|directory| directory.join(if cfg!(windows) { "node.exe" } else { "node" }));
            let executable = bundled_node
                .filter(|path| path.is_file())
                .unwrap_or_else(|| PathBuf::from("node"));
            let mut process = Command::new(executable);
            process
                .arg("--permission")
                .arg(format!("--allow-fs-read={}", host_path.display()))
                .arg(&host_path)
                .env_clear()
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit());
            if let Some(path) = std::env::var_os("PATH") {
                process.env("PATH", path);
            }
            if let Some(root) = std::env::var_os("SystemRoot") {
                process.env("SystemRoot", root);
            }
            match process.spawn() {
                Ok(mut started) => {
                    let Some(stdout) = started.stdout.take() else {
                        let _ = publish(error_event("runtime output pipe is missing".to_owned()));
                        let _ = started.kill();
                        let _ = started.wait();
                        continue;
                    };
                    let (reply_tx, reply_rx) = mpsc::channel();
                    thread::spawn(move || read_replies(stdout, reply_tx));
                    child = Some(HostProcess {
                        child: started,
                        replies: reply_rx,
                    });
                }
                Err(error) => {
                    let _ = publish(error_event(format!("cannot start Node runtime: {error}")));
                    continue;
                }
            }
        }
        let Some(running) = child.as_mut() else {
            let _ = publish(if command["type"] == "stop" {
                serde_json::json!({"type":"stopped"})
            } else {
                error_event("no cartridge runtime is active".to_owned())
            });
            continue;
        };
        match exchange(running, &command) {
            Ok(reply) => {
                let faulted = reply["type"] == "error";
                let _ = publish(reply);
                if faulted {
                    if let Some(failed) = child.take() {
                        failed.stop();
                    }
                }
            }
            Err(error) => {
                let _ = publish(error_event(error));
                if let Some(failed) = child.take() {
                    failed.stop();
                }
            }
        }
        if command["type"] == "stop" || command["type"] == "render-wav" {
            if let Some(stopped) = child.take() {
                stopped.stop();
            }
        }
    }
    if let Some(running) = child {
        running.stop();
    }
}

fn exchange(running: &mut HostProcess, command: &Value) -> Result<Value, String> {
    let stdin = running.child.stdin.as_mut().ok_or("runtime input closed")?;
    serde_json::to_writer(&mut *stdin, command).map_err(|error| error.to_string())?;
    stdin.write_all(b"\n").map_err(|error| error.to_string())?;
    stdin.flush().map_err(|error| error.to_string())?;
    running
        .replies
        .recv_timeout(COMMAND_TIMEOUT)
        .map_err(|_| "runtime did not respond within five seconds".to_owned())?
}

fn read_replies(stdout: ChildStdout, replies: Sender<Result<Value, String>>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        let result = reader
            .by_ref()
            .take(MAX_RESPONSE_BYTES as u64 + 1)
            .read_line(&mut line)
            .map_err(|error| error.to_string())
            .and_then(|length| {
                if length == 0 {
                    Err("runtime process exited".to_owned())
                } else if length > MAX_RESPONSE_BYTES {
                    Err("runtime response exceeded limit".to_owned())
                } else {
                    serde_json::from_str(&line)
                        .map_err(|error| format!("runtime response invalid: {error}"))
                }
            });
        let failed = result.is_err();
        if replies.send(result).is_err() || failed {
            return;
        }
    }
}

fn error_event(message: String) -> Value {
    serde_json::json!({ "type": "error", "message": message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn queued_frame_save_audio_and_stop_replies_cannot_cross_runtime_sessions() {
        let (commands, received) = mpsc::channel();
        let (events, replies) = mpsc::channel();
        let bridge = RuntimeBridge {
            sender: commands,
            receiver: replies,
            generation: Arc::new(AtomicU64::new(0)),
            pending: Cell::new(0),
        };
        events
            .send((
                Some(0),
                json!({"type":"frame","saveCommit":"old","left":"old","right":"old"}),
            ))
            .unwrap();
        bridge.send(json!({"type":"load"})).unwrap();
        assert_eq!(received.recv().unwrap(), (1, json!({"type":"load"})));
        events.send((Some(0), json!({"type":"stopped"}))).unwrap();
        events
            .send((Some(1), json!({"type":"loaded","cartridgeId":"new"})))
            .unwrap();
        assert_eq!(
            bridge.poll().unwrap(),
            json!({"type":"loaded","cartridgeId":"new"})
        );
        assert!(bridge.poll().is_none());
        bridge.send(json!({"type":"memory"})).unwrap();
        bridge.send(json!({"type":"snapshot"})).unwrap();
        assert_eq!(received.recv().unwrap().0, 1);
        assert_eq!(received.recv().unwrap().0, 1);
        events.send((Some(1), json!({"type":"memory"}))).unwrap();
        assert_eq!(bridge.poll().unwrap()["type"], "memory");
        assert!(bridge.has_pending());
        events.send((Some(1), json!({"type":"snapshot"}))).unwrap();
        assert_eq!(bridge.poll().unwrap()["type"], "snapshot");
        assert!(!bridge.has_pending());
        bridge.send(json!({"type":"stop"})).unwrap();
        assert_eq!(received.recv().unwrap().0, 2);
        events
            .send((Some(1), json!({"type":"snapshot","snapshot":"old"})))
            .unwrap();
        events.send((Some(2), json!({"type":"stopped"}))).unwrap();
        assert_eq!(bridge.poll().unwrap(), json!({"type":"stopped"}));
        assert!(bridge.poll().is_none());
        events
            .send((None, error_event("missing runtime".to_owned())))
            .unwrap();
        assert_eq!(bridge.poll().unwrap()["type"], "error");
    }

    fn receive(runtime: &RuntimeBridge) -> Value {
        let deadline = Instant::now() + Duration::from_secs(7);
        loop {
            if let Some(event) = runtime.poll() {
                return event;
            }
            assert!(Instant::now() < deadline, "native bridge timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn rapid_real_cartridge_reloads_only_deliver_the_latest_machine_and_its_save_commit() {
        let project = |id: &str, color: u8| {
            let mut project =
                modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w"))
                    .unwrap();
            project.manifest.id = id.to_owned();
            project.files.insert("src/main.modl".to_owned(),format!("on start:\n  save_set_int(\"owner\", {color})\n  save_commit()\n\non draw:\n  clear({color})\n").into_bytes());
            let source = modl_core::encode_workcart(
                &project.manifest,
                &project.files,
                &project.recipes,
                &project.tests,
            )
            .unwrap();
            crate::project::NativeProject::from_source(source, None).unwrap()
        };
        let first = project("epoch-first", 7);
        let second = project("epoch-second", 9);
        let bridge = RuntimeBridge::new(crate::native_host_path());
        bridge
            .send(first.load_command(false, &[]).unwrap())
            .unwrap();
        bridge
            .send(second.load_command(false, &[]).unwrap())
            .unwrap();
        assert_eq!(
            receive(&bridge),
            json!({"type":"loaded","cartridgeId":"epoch-second"})
        );
        bridge
            .send(json!({"type":"frame","input":crate::replay::empty_input()}))
            .unwrap();
        let frame = receive(&bridge);
        assert_eq!(frame["type"], "frame");
        assert!(
            STANDARD
                .decode(frame["pixels"].as_str().unwrap())
                .unwrap()
                .iter()
                .all(|pixel| *pixel == 9)
        );
        assert!(frame["saveCommit"].is_string());
        bridge
            .send(json!({"type":"frame","input":crate::replay::empty_input()}))
            .unwrap();
        bridge
            .send(first.load_command(false, &[]).unwrap())
            .unwrap();
        assert_eq!(
            receive(&bridge),
            json!({"type":"loaded","cartridgeId":"epoch-first"})
        );
        bridge
            .send(json!({"type":"frame","input":crate::replay::empty_input()}))
            .unwrap();
        let frame = receive(&bridge);
        assert_eq!(frame["type"], "frame");
        assert!(
            STANDARD
                .decode(frame["pixels"].as_str().unwrap())
                .unwrap()
                .iter()
                .all(|pixel| *pixel == 7)
        );
        assert!(frame["saveCommit"].is_string());
        bridge.send(json!({"type":"stop"})).unwrap();
        assert_eq!(receive(&bridge)["type"], "stopped");
    }

    #[test]
    fn missing_runtime_startup_errors_remain_visible_after_a_session_change() {
        let bridge = RuntimeBridge::new(std::env::temp_dir().join(format!(
            "missing-mod01-native-host-{}.mjs",
            std::process::id()
        )));
        bridge.send(json!({"type":"load"})).unwrap();
        let event = receive(&bridge);
        assert_eq!(event["type"], "error");
        assert!(
            event["message"]
                .as_str()
                .unwrap()
                .contains("native runtime bundle is missing")
        );
    }

    #[test]
    fn live_audio_preview_runs_in_an_isolated_native_child_and_restarts_its_clock() {
        let runtime = RuntimeBridge::new(crate::native_host_path());
        let request = |command| {
            runtime.send(command).unwrap();
            let deadline = Instant::now() + Duration::from_secs(7);
            loop {
                if let Some(event) = runtime.poll() {
                    return event;
                }
                assert!(Instant::now() < deadline, "live audio preview timed out");
                thread::sleep(Duration::from_millis(1));
            }
        };
        let load = json!({"type":"audio-preview-load","assets":[{
            "kind":"sound","name":"tone","waveform":"triangle","note":60,
            "durationFrames":4,"volume":0.5,"pan":0,
            "envelope":{"attackFrames":0,"decayFrames":0,"sustainLevel":1,"releaseFrames":1},
            "pitch":{"slideSemitonesPerFrame":0,"vibratoDepthSemitones":0,"vibratoPeriodFrames":0}
        }]});
        assert_eq!(request(load.clone())["type"], "audio-preview-loaded");
        assert_eq!(
            request(json!({"type":"audio-preview-audition","sound":"tone","note":64,"volume":0.5}))
                ["type"],
            "audio-preview-auditioned"
        );
        let frame = request(json!({"type":"audio-preview-frame"}));
        assert_eq!(frame["type"], "audio-preview-frame");
        assert_eq!(frame["frame"], 0);
        assert_eq!(frame["voices"][0]["note"], 64);
        let left = STANDARD.decode(frame["left"].as_str().unwrap()).unwrap();
        assert_eq!(left.len(), 800 * 4);
        assert!(left.iter().any(|byte| *byte != 0));
        assert!(frame.get("saveCommit").is_none());
        assert!(frame["position"].is_null());
        assert_eq!(request(json!({"type":"stop"}))["type"], "stopped");
        assert_eq!(request(load)["type"], "audio-preview-loaded");
        let fresh = request(json!({"type":"audio-preview-frame"}));
        assert_eq!(fresh["frame"], 0);
        assert_eq!(fresh["activeVoices"], 0);
        assert_eq!(request(json!({"type":"stop"}))["type"], "stopped");
    }
}
