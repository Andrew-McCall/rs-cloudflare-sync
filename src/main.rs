use std::fs::File;
use std::io::{self, Read, BufReader, BufRead};
use std::env;
use std::process::{Command, exit};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

fn get_public_ip() -> Result<String, io::Error> {
    let curl = Command::new("curl")
        .arg("https://api.ipify.org")
        .output()?;

    if curl.stdout.is_empty() {
        return Err(io::Error::new(io::ErrorKind::Other, format!("Curl command failed: {}", String::from_utf8_lossy(&curl.stderr))));
    }

    match String::from_utf8(curl.stdout) {
        Ok(ip) => Ok(ip),
        Err(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 response")),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareConfig {
    r#type: String,    // Field for the type of DNS record (e.g., "A", "CNAME", etc.)
    name: String,      // Field for the name of the DNS record (e.g., "www.example.com")
    content: String,   // Field for the ip of the DNS record
    ttl: i32,          // Field for the TTL (Time-to-Live) value in second (1 means Automatic)
    proxied: bool,     // Field to indicate if the record is proxied through Cloudflare (true/false)
}

impl Default for CloudflareConfig {
    fn default() -> Self {
        CloudflareConfig {
            r#type: "A".to_string(),
            name: "www.example.com".to_string(),
            content: "x.x.x.x".to_string(),
            ttl: 1,
            proxied: false
        }
    }
}

#[derive(Debug)]
struct NginxConfig {
    internal_port: String,
    external_port: String,
    ssl: bool,
    domain: String, 
}

impl Default for NginxConfig {
    fn default() -> Self {
        NginxConfig{
            internal_port: "80".to_string(),
            external_port: "80".to_string(),
            ssl: false,
            domain: "www.example.com".to_string(),
        }
    }
}

fn read_config_line(line: &str) -> Result<NginxConfig, Box<dyn std::error::Error>>{
    let mut config = NginxConfig::default();

    let split: Vec<&str> = line.split_whitespace().collect();   
     if split.len() != 3 {
        return Err("Invalid configuration: expected 3 fields: expected DOMAIN INTERNAL:EXTERNAL SSL".into());
    } 

    config.domain = split[0].to_owned();

    let port_split: Vec<&str> = split[1].split(':').collect();
    if port_split.len() != 2 {
        return Err("Invalid port mapping: expected external:internal".into());
    }

    config.external_port = port_split[0].to_string();
    config.internal_port = port_split[1].to_string();

    config.ssl = match split[2] {
        "1" => true,
        "0" => false,
        _ => return Err("Invalid SSL flag: expected '1' or '0'".into()),
    };

    Ok(config) 
}

fn read_config(file_path: &str) -> std::io::Result<Vec<NginxConfig>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut configs = Vec::new();
    for line in reader.lines() {
        let line = line?;
        match read_config_line(&line) {
            Ok(config) => configs.push(config),
            Err(e) => eprintln!("Error reading config line: {}\n({})", e, &line),
        }
    }

    Ok(configs)
}

fn get_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

#[derive(Debug, Serialize, Deserialize)]
struct Secret {
    nginx_output: String,
    cloudflare_api_key: String,
    cloudflare_email: String,
    cloudflare_zone_id: String,
    config_path: String,
    last_ip: String,
    last_ran_timestamp: i128,
}

impl Default for Secret {
    fn default() -> Self {
        Secret {
            config_path: "path/to/config".to_string(),
            last_ip:"x.x.x.x".to_string(),
            nginx_output: "path/to/nginx".to_string(),
            cloudflare_api_key: "API_KEY".to_string(),
            cloudflare_email: "example@a.bc".to_string(),
            cloudflare_zone_id: "ZONE_ID".to_string(),
            last_ran_timestamp: -1,
        }
    }
}

fn read_secrets(file_path: &str) -> Result<Secret, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let secrets: Secret = serde_json::from_str(&contents).expect("Failed to parse JSON");
    
    println!("nginx_output: {}", secrets.nginx_output);
    println!("cloudflare_api_key: {}", secrets.cloudflare_api_key);

    Ok(secrets)
}

fn write_secrets(file_path: &str, secrets: &Secret) -> std::io::Result<()> {
    let file = File::create(file_path)?;
    let writer = std::io::BufWriter::new(file);

    serde_json::to_writer(writer, secrets).expect("Failed to write JSON");

    Ok(())
}


fn main() {
    let args : Vec<String> = env::args().collect();

    let secret_path;
    if args.len() < 2 {
        secret_path = ".secrets";
    }else{
        secret_path = &args[1];
        if secret_path == "default" {
            let _ = write_secrets(".secrets", &Secret::default());
        }
    }

    let mut secrets = match read_secrets(secret_path) {
        Ok(secrets) => secrets,
        Err(e) => {
            eprintln!("Error reading config file: {}", e);
            exit(1);
        }
    };

    let public_ip = match get_public_ip() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Error curling Ip: {}", e);
            exit(1);
        }
    };

    if public_ip == secrets.last_ip {
        println!("Ip hasn't changed");
        exit(0);
    } 

    println!("Ip: {}", public_ip);

    secrets.last_ip = public_ip;

    let nconfigs = match read_config(&secrets.config_path) {
        Ok(configs) => configs,
        Err(e) => {
            eprintln!("Error reading config file: {}", e);
            exit(1);
        }
    };

} 
