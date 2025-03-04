use crate::log::Log;
use crate::saml_server::Saml;
use crate::LocalConfig;
use lazy_static::lazy_static;
use std::env;
use std::ffi::OsString;
use std::fs::{create_dir_all, remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use temp_dir::TempDir;
use tokio::io::AsyncBufReadExt;

// Change from a relative path to a temp file path
lazy_static! {
    static ref SHARED_DIR: String = {
        let path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(env::temp_dir()))
            .join("openaws-vpn-client");

        // Create the directory if it doesn't exist
        if !path.exists() {
            create_dir_all(&path).unwrap_or_else(|e| {
                eprintln!("Failed to create shared directory: {}", e);
            });
        }

        path.to_string_lossy().to_string()
    };

static ref DEFAULT_PWD_FILE: String = {
    let path = Path::new(&*SHARED_DIR).join("pwd.txt");

    // Create properly formatted file if it doesn't exist
    if let Ok(mut file) = File::create(&path) {
        // OpenVPN requires two non-empty lines: username and password
        writeln!(file, "placeholder_username").unwrap_or_else(|e| eprintln!("Error writing username: {}", e));
        writeln!(file, "placeholder_password").unwrap_or_else(|e| eprintln!("Error writing password: {}", e));
    }

    path.to_string_lossy().to_string()
};

    static ref OPENVPN_FILE: String = {
         let custom_path = "/Users/niteshchowdharybalusu/Downloads/openaws-vpn-client/share/openvpn/bin/openvpn";

    if Path::new(custom_path).exists() {
        return custom_path.to_string();
    }


        // Try to find openvpn in common locations
        let possible_paths = vec![
            "/usr/bin/openvpn",
            "/usr/local/bin/openvpn",
            "/opt/homebrew/bin/openvpn", // Common on macOS with Homebrew
            "/opt/local/bin/openvpn",    // Common on macOS with MacPorts
            "C:\\Program Files\\OpenVPN\\bin\\openvpn.exe", // Windows path
        ];

        for path in &possible_paths {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }

        // Fall back to hoping it's in PATH
        "openvpn".to_string()
    };
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

pub async fn run_ovpn(log: Arc<Log>, config: PathBuf, addr: String, port: u16) -> AwsSaml {
    // Log the paths we're using for debugging
    log.append(format!("Using shared directory: {}", SHARED_DIR.as_str()));
    log.append(format!("Using pwd file: {}", DEFAULT_PWD_FILE.as_str()));
    log.append(format!("Using OpenVPN path: {}", OPENVPN_FILE.as_str()));

    if !Path::new(OPENVPN_FILE.as_str()).exists() {
        log.append(format!(
            "WARNING: OpenVPN executable not found at '{}'",
            OPENVPN_FILE.as_str()
        ));
    }

    // Create the command
    let mut cmd = tokio::process::Command::new(OPENVPN_FILE.as_str());
    cmd.arg("--config")
        .arg(&config)
        .arg("--verb")
        .arg("3")
        .arg("--proto")
        .arg("udp")
        .arg("--remote")
        .arg(addr)
        .arg(format!("{}", port))
        .arg("--auth-user-pass")
        .arg(DEFAULT_PWD_FILE.as_str())
        .stdout(Stdio::piped())
        .current_dir(SHARED_DIR.as_str());

    // Log the full command for debugging
    log.append(format!("Executing command: {:?}", cmd));

    // Try to spawn the process, but handle errors gracefully
    let out = match cmd.spawn() {
        Ok(o) => o,
        Err(e) => {
            log.append(format!("Error starting OpenVPN: {}", e));
            panic!("Failed to start OpenVPN: {}", e);
        }
    };

    let pid = out.id().unwrap_or(0);
    let stdout = match out.stdout {
        Some(s) => s,
        None => {
            log.append("Failed to capture OpenVPN stdout");
            panic!("Failed to capture OpenVPN stdout");
        }
    };

    let buf = tokio::io::BufReader::new(stdout);
    let log = log.clone();
    let mut lines = buf.lines();

    let mut next = lines.next_line().await;
    let mut addr = None::<String>;
    let mut pwd = None::<String>;

    loop {
        if let Ok(ref line) = next {
            if let Some(line) = line {
                log.append_process(pid, line.as_str());

                // Check for different types of auth failures that might contain SAML info
                let auth_prefix_crv1 = "AUTH_FAILED,CRV1";
                let auth_prefix_simple = "AUTH_FAILED";
                let prefix = "https://";

                if line.contains(auth_prefix_crv1) {
                    log.append_process(pid, format!("Found SAML auth redirect: {}", line).as_str());
                    if let Some(find) = line.find(prefix) {
                        addr = Some((&line[find..]).to_string());

                        if let Some(auth_find) = line
                            .find(auth_prefix_crv1)
                            .map(|v| v + auth_prefix_crv1.len() + 1)
                        {
                            if auth_find < find {
                                let sub = &line[auth_find..find - 1];
                                if let Some(e) = sub.split(":").skip(1).next() {
                                    pwd = Some(e.to_string());
                                }
                            }
                        }
                    }
                }
                // Fix the variable name issue:
                else if line.contains(auth_prefix_simple) {
                    // Try to open the AWS Client VPN portal directly
                    log.append_process(
                        pid,
                        "Regular auth failure detected, trying direct portal access",
                    );

                    // Extract domain from config file or use a default
                    let config_path = config.to_string_lossy();
                    let portal_url = if config_path.contains("cvpn-endpoint") {
                        // Try to extract the endpoint ID from the config path
                        let parts: Vec<&str> = config_path.split("cvpn-endpoint").collect();
                        if parts.len() > 1 {
                            format!("https://self-service.clientvpn.amazonaws.com/endpoints/cvpn-endpoint{}", parts[1].split('.').next().unwrap_or(""))
                        } else {
                            "https://self-service.clientvpn.amazonaws.com/".to_string()
                        }
                    } else {
                        // If we can't determine the domain, use a generic URL
                        "https://self-service.clientvpn.amazonaws.com/".to_string()
                    };

                    log.append_process(pid, format!("Opening browser to: {}", portal_url).as_str());
                    addr = Some(portal_url);
                    pwd = Some("direct-portal".to_string());
                }
            } else {
                break;
            }
        } else {
            break;
        }

        next = lines.next_line().await;
    }

    // If we didn't get authentication info, print a helpful message
    if addr.is_none() || pwd.is_none() {
        log.append("Failed to extract SAML URL or password from OpenVPN output");
        log.append("This may be because:");
        log.append(
            "1. Your VPN config file doesn't have the correct SAML authentication directives",
        );
        log.append("2. The VPN server isn't configured for SAML authentication");
        log.append("Try adding these lines to your .ovpn file:");
        log.append("auth-federate");
        log.append("auth-user-pass");
        log.append("auth-retry interact");

        // Provide a direct URL to try manually
        let hostname = addr.unwrap_or_else(|| "your-domain.com".to_string());
        log.append(format!("You can try manually opening: https://self-service.clientvpn.amazonaws.com/endpoints/{}", hostname));

        panic!("Failed to extract SAML URL or password");
    }

    AwsSaml {
        url: addr.unwrap(),
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
    let temp = TempDir::new().unwrap();
    let temp_pwd = temp.child("pwd.txt");

    if temp_pwd.exists() {
        remove_file(&temp_pwd).unwrap_or_else(|e| {
            log.append(format!("Failed to remove temp file: {}", e));
        });
    }

    println!(
        "Temp pwd file before saving: {}",
        temp_pwd.to_string_lossy()
    );

    let mut save = match File::create(&temp_pwd) {
        Ok(f) => f,
        Err(e) => {
            log.append(format!("Failed to create temp pwd file: {}", e));
            panic!("Failed to create temp pwd file: {}", e);
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_pwd).unwrap().permissions();
        perms.set_mode(0o600); // User read/write, no group/other access
        std::fs::set_permissions(&temp_pwd, perms).unwrap();
    }

    println!("Temp pwd file: {}", temp_pwd.to_string_lossy());

    write!(save, "SAML\nCRV1::{}::{}\n", saml.pwd, saml.data).unwrap_or_else(|e| {
        println!("Failed to write to temp pwd file: {}", e);
        log.append(format!("Failed to write to temp pwd file: {}", e));
    });

    log.append(format!(
        "SAML auth string: CRV1::{}::{}",
        saml.pwd.len(),
        saml.data.len()
    ));

    let b = match std::fs::canonicalize(&temp_pwd) {
        Ok(p) => p,
        Err(e) => {
            log.append(format!("Failed to canonicalize temp pwd path: {}", e));
            temp_pwd.to_path_buf()
        }
    };

    // Check for sudo or pkexec
    let sudo_cmd = if cfg!(target_os = "macos") {
        // macOS typically uses sudo
        "sudo"
    } else {
        // Linux typically has pkexec, but fall back to sudo
        if Path::new("/usr/bin/pkexec").exists() {
            "pkexec"
        } else {
            "sudo"
        }
    };

    log.append(format!("Using privilege escalation command: {}", sudo_cmd));

    let mut cmd = tokio::process::Command::new(sudo_cmd);
    cmd.arg(OPENVPN_FILE.as_str())
        .arg("--config")
        .arg(&config)
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
        .arg(rm_file_command(&b))
        .arg("--auth-user-pass")
        .arg(&b)
        .stdout(Stdio::piped())
        .current_dir(SHARED_DIR.as_str())
        .kill_on_drop(true);

    // Log the full command for debugging
    log.append(format!("Executing connection command: {:?}", cmd));

    let mut out = match cmd.spawn() {
        Ok(o) => o,
        Err(e) => {
            log.append(format!("Failed to start OpenVPN connection: {}", e));
            panic!("Failed to start OpenVPN connection: {}", e);
        }
    };

    let pid = out.id().unwrap_or(0);
    // Set pid
    {
        let mut stored_pid = process_info.pid.lock().unwrap();
        *stored_pid = Some(pid);
        LocalConfig::save_last_pid(Some(pid));
    }

    let stdout = match out.stdout.take() {
        Some(s) => s,
        None => {
            log.append("Failed to capture OpenVPN connection stdout");
            panic!("Failed to capture OpenVPN connection stdout");
        }
    };

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

    match out.wait().await {
        Ok(status) => status.code().unwrap_or(-1),
        Err(e) => {
            log.append(format!("Error waiting for OpenVPN process: {}", e));
            -1
        }
    }
}

pub fn kill_openvpn(pid: u32) {
    if pid == 0 || pid == 1 {
        LocalConfig::save_last_pid(None);
        return;
    }

    let info = match Command::new("ps")
        .arg("-o")
        .arg("cmd")
        .arg("-p")
        .arg(format!("{}", pid))
        .output()
    {
        Ok(output) => output,
        Err(_) => {
            println!("Failed to execute ps command to check pid {}", pid);
            LocalConfig::save_last_pid(None);
            return;
        }
    };

    if let Ok(msg) = String::from_utf8(info.stdout) {
        let last = msg.lines().rev().next();
        if let Some(last) = last {
            if last.len() > 0 && last.chars().next().map(|v| v == '/').unwrap_or(false) {
                if last.contains("openvpn --config /")
                    && last.contains("--auth-user-pass /")
                    && last.ends_with("pwd.txt")
                {
                    // Check for sudo or pkexec
                    let sudo_cmd = if cfg!(target_os = "macos") {
                        "sudo"
                    } else if Path::new("/usr/bin/pkexec").exists() {
                        "pkexec"
                    } else {
                        "sudo"
                    };

                    let mut p = Command::new(sudo_cmd)
                        .arg("kill")
                        .arg(format!("{}", pid))
                        .spawn()
                        .unwrap_or_else(|e| {
                            println!("Failed to kill OpenVPN process {}: {}", pid, e);
                            panic!("Failed to kill OpenVPN process {}: {}", pid, e);
                        });

                    let _ = p.wait();
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
    if cfg!(target_os = "windows") {
        str.push("cmd.exe /c del ");
    } else {
        str.push("/usr/bin/env rm ");
    }
    str.push(dir);
    str
}
