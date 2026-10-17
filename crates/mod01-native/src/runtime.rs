use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdout, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
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
    sender: Sender<Value>,
    receiver: Receiver<Value>,
}

impl RuntimeBridge {
    pub fn new(host_path: PathBuf) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        thread::spawn(move || worker(host_path, command_rx, event_tx));
        Self {
            sender: command_tx,
            receiver: event_rx,
        }
    }

    pub fn send(&self, command: Value) -> Result<(), String> {
        self.sender.send(command).map_err(|error| error.to_string())
    }

    pub fn poll(&self) -> Option<Value> {
        self.receiver.try_recv().ok()
    }
}

fn worker(host_path: PathBuf, commands: Receiver<Value>, events: Sender<Value>) {
    let host_path = match host_path.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            let _ = events.send(error_event(format!(
                "native runtime bundle is missing: {error}"
            )));
            return;
        }
    };
    let mut child: Option<HostProcess> = None;
    for command in commands {
        if command["type"] == "load" {
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
                        let _ =
                            events.send(error_event("runtime output pipe is missing".to_owned()));
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
                    let _ = events.send(error_event(format!("cannot start Node runtime: {error}")));
                    continue;
                }
            }
        }
        let Some(running) = child.as_mut() else {
            let _ = events.send(if command["type"] == "stop" {
                serde_json::json!({"type":"stopped"})
            } else {
                error_event("no cartridge runtime is active".to_owned())
            });
            continue;
        };
        match exchange(running, &command) {
            Ok(reply) => {
                let faulted = reply["type"] == "error";
                let _ = events.send(reply);
                if faulted {
                    if let Some(failed) = child.take() {
                        failed.stop();
                    }
                }
            }
            Err(error) => {
                let _ = events.send(error_event(error));
                if let Some(failed) = child.take() {
                    failed.stop();
                }
            }
        }
        if command["type"] == "stop" {
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
