use crate::local_config::LocalConfig;
use crate::log::Log;
use crate::saml_server::Saml;
use lazy_static::lazy_static;
use std::env;
use std::ffi::OsString;
use std::fs::{remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncBufReadExt;

const DEFAULT_PWD_FILE: &str = "./pwd.txt";

lazy_static! {
    static ref SHARED_DIR: String = std::env::var("SHARED_DIR").unwrap_or("./share".to_string());
    static ref OPENVPN_FILE: String =
        std::env::var("OPENVPN_FILE").unwrap_or("./openvpn/bin/openvpn".to_string());
}

pub struct ProcessInfo {
    pub pid: Mutex<Option<u32>>,
}

impl ProcessInfo {
    pub fn new() -> Self {
        Self {
            pid: Mutex::new(None),
        }
    }
}

#[derive(Debug)]
pub struct AwsSaml {
    pub url: String,
    pub pwd: String,
}

// Modify the run_ovpn function in cmd.rs to ensure proper remote addressing
pub async fn run_ovpn(log: Arc<Log>, config: PathBuf, addr: String, port: u16) -> AwsSaml {
    let path = Path::new(SHARED_DIR.as_str()).join(DEFAULT_PWD_FILE);
    if !path.exists() {
        println!(
            "{:?} does not exist in {:?}!",
            path,
            env::current_dir().unwrap()
        );
    }

    // Log the connection attempt
    log.append(format!("Connecting to: {} port {}", addr, port).as_str());

    let out = tokio::process::Command::new(OPENVPN_FILE.as_str())
        .arg("--config")
        .arg(config)
        .arg("--verb")
        .arg("3")
        .arg("--proto")
        .arg("udp")
        .arg("--remote")
        .arg(addr)
        .arg(format!("{}", port))
        .arg("--auth-user-pass")
        .arg(DEFAULT_PWD_FILE)
        .stdout(Stdio::piped())
        .current_dir(SHARED_DIR.as_str())
        .spawn()
        .unwrap();

    let pid = out.id().unwrap();
    let stdout = out.stdout.unwrap();

    let buf = tokio::io::BufReader::new(stdout);
    let log = log.clone();
    let mut lines = buf.lines();
    let mut next = lines.next_line().await;
    let mut url = None::<String>;
    let mut pwd = None::<String>;

    while let Ok(Some(line)) = next {
        log.append_process(pid, line.as_str());

        // Look for the AUTH_FAILED message with SAML URL
        if line.contains("AUTH_FAILED,CRV1") {
            log.append_process(pid, format!("Found AUTH redirect: {}", line).as_str());

            // Extract the instance ID which is the password
            if let Some(start_idx) = line.find("CRV1:R:") {
                let start_idx = start_idx + "CRV1:R:".len();
                if let Some(end_idx) = line[start_idx..].find(":") {
                    let instance_id = &line[start_idx..(start_idx + end_idx)];
                    pwd = Some(instance_id.to_string());
                    log.append(format!("Extracted SAML password: {}", instance_id).as_str());
                }
            }

            // Extract the URL
            if let Some(url_idx) = line.find("https://") {
                url = Some(line[url_idx..].to_string());
            }

            // If we found both, we can stop
            if url.is_some() && pwd.is_some() {
                break;
            }
        }

        next = lines.next_line().await;
    }

    // Handle case where auth info wasn't found
    if url.is_none() || pwd.is_none() {
        log.append("Error: Failed to extract SAML authentication info");
        return AwsSaml {
            url: "Error: No authentication URL received".to_string(),
            pwd: "Error: No password received".to_string(),
        };
    }

    // Log what we found for debugging
    log.append(format!("Found SAML URL: {}", url.as_ref().unwrap()).as_str());
    log.append(format!("Found SAML password: {}", pwd.as_ref().unwrap()).as_str());

    AwsSaml {
        url: url.unwrap(),
        pwd: pwd.unwrap(),
    }
}

pub async fn connect_ovpn(
    log: Arc<Log>,
    config: PathBuf,
    addr: String,
    port: u16,
    saml: Saml,
    process_info: Arc<ProcessInfo>,
) -> i32 {
    let temp_dir = std::env::temp_dir();
    let temp_pwd = temp_dir.join("ovpn_pwd.txt");

    log.append(format!("Using password file at: {:?}", temp_pwd).as_str());

    // Remove existing file
    if temp_pwd.exists() {
        let _ = remove_file(&temp_pwd);
    }

    // Create with secure permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        match File::options()
            .write(true)
            .create(true)
            .mode(0o600) // Owner-only permissions
            .open(&temp_pwd)
        {
            Ok(mut file) => {
                if let Err(e) = write!(file, "N/A\nCRV1::{}::{}\n", saml.pwd, saml.data) {
                    log.append(format!("Error writing to password file: {}", e).as_str());
                    return -1;
                }
            }
            Err(e) => {
                log.append(format!("Error creating password file: {}", e).as_str());
                return -1;
            }
        }
    }

    #[cfg(not(unix))]
    {
        // Non-Unix implementation
        match File::create(&temp_pwd) {
            Ok(mut file) => {
                if let Err(e) = write!(file, "N/A\nCRV1::{}::{}\n", saml.pwd, saml.data) {
                    log.append(format!("Error writing to password file: {}", e).as_str());
                    return -1;
                }
            }
            Err(e) => {
                log.append(format!("Error creating password file: {}", e).as_str());
                return -1;
            }
        }
    }

    // Rest of your connect_ovpn function
    let pwd_path = match std::fs::canonicalize(&temp_pwd) {
        Ok(path) => path,
        Err(e) => {
            log.append(format!("Error canonicalizing password file path: {}", e).as_str());
            return -1;
        }
    };

    // Continue with OpenVPN command execution...
    log.append(format!("Connecting to {} port {} with SAML data", addr, port).as_str());

    let mut out = tokio::process::Command::new("pkexec")
        .arg(OPENVPN_FILE.as_str())
        .arg("--config")
        .arg(config)
        .arg("--verb")
        .arg("3")
        .arg("--auth-nocache")
        .arg("--inactive")
        .arg("3600")
        .arg("--proto")
        .arg("udp")
        .arg("--remote")
        .arg(addr)
        .arg(format!("{}", port))
        .arg("--script-security")
        .arg("2")
        .arg("--route-up")
        .arg(rm_file_command(&pwd_path))
        .arg("--auth-user-pass")
        .arg(pwd_path)
        .stdout(Stdio::piped())
        .current_dir(SHARED_DIR.as_str())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let pid = out.id().unwrap();
    // Set pid
    {
        let mut stored_pid = process_info.pid.lock().unwrap();
        *stored_pid = Some(pid);
        LocalConfig::save_last_pid(Some(pid));
    }

    let stdout = out.stdout.take().unwrap();

    let buf = tokio::io::BufReader::new(stdout);
    let log = log.clone();
    let mut lines = buf.lines();

    let mut next = lines.next_line().await;

    loop {
        if let Ok(ref line) = next {
            if let Some(line) = line {
                log.append_process(pid, line.as_str());
            } else {
                break;
            }
        } else {
            break;
        }

        next = lines.next_line().await;
    }

    out.wait().await.unwrap().code().unwrap()
}

pub fn kill_openvpn(pid: u32) {
    if pid == 0 || pid == 1 {
        LocalConfig::save_last_pid(None);
        return;
    }

    let info = Command::new("ps")
        .arg("-o")
        .arg("cmd")
        .arg("-p")
        .arg(format!("{}", pid))
        .output()
        .unwrap();

    if let Ok(msg) = String::from_utf8(info.stdout) {
        let last = msg.lines().rev().next();
        if let Some(last) = last {
            if last.len() > 0 && last.chars().next().map(|v| v == '/').unwrap_or(false) {
                if last.contains("openvpn --config /")
                    && last.contains("--auth-user-pass /")
                    && last.ends_with("pwd.txt")
                {
                    let mut p = Command::new("pkexec")
                        .arg("kill")
                        .arg(format!("{}", pid))
                        .spawn()
                        .unwrap();

                    p.wait().unwrap();
                    LocalConfig::save_last_pid(None);
                } else {
                    LocalConfig::save_last_pid(None);
                }
            } else {
                LocalConfig::save_last_pid(None);
            }
        }
    }
}

fn rm_file_command(dir: &PathBuf) -> OsString {
    let mut str = OsString::new();
    str.push("/usr/bin/env rm ");
    str.push(dir);
    str
}
