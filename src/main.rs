use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::process::{Command, exit};

const FILE_HEADER: &str = "file:";
fn remove_file_header(string: &str) -> &str {
    &string[FILE_HEADER.len()..]
}

struct Logger(Option<BufWriter<File>>);

impl Logger {
    fn open(path: Option<&str>) -> Self {
        Logger(path.and_then(|p| {
            File::options()
                .create(true)
                .append(true)
                .open(p)
                .ok()
                .map(BufWriter::new)
        }))
    }

    fn log(&mut self, msg: &str) {
        println!("{}", msg);
        if let Some(f) = &mut self.0 {
            let _ = writeln!(f, "{}", msg);
        }
    }
}

fn execute(command: &mut Command) -> Result<String, io::Error> {
    let output = command.output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    match String::from_utf8(output.stdout) {
        Ok(result) => Ok(result),
        Err(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid UTF-8 response",
        )),
    }
}

fn get_public_ip() -> Result<String, io::Error> {
    execute(Command::new("curl").arg("https://api.ipify.org"))
}

#[derive(Debug, Serialize, Deserialize)]
struct APIResult {
    result: Vec<CloudflareAPI>,
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareAPI {
    id: String,
    name: String,
    content: Option<String>,
    comment: Option<String>,
    r#type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Secret {
    cloudflare_api_key: String,
    last_ip: Option<String>,
    log_path: Option<String>,

    #[serde(skip)]
    is_file: bool,
}

impl Default for Secret {
    fn default() -> Self {
        Secret {
            last_ip: None,
            cloudflare_api_key: "API_KEY".to_string(),
            log_path: None,
            is_file: false,
        }
    }
}

impl Secret {
    fn new(cloudflare_api_key: &str) -> Self {
        Secret {
            cloudflare_api_key: cloudflare_api_key.to_string(),
            ..Default::default()
        }
    }
}

fn read_secrets(file_path: &str) -> Result<Secret, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let mut secrets: Secret = serde_json::from_str(&contents).expect("Failed to parse JSON");
    secrets.is_file = true;

    Ok(secrets)
}

fn write_secrets(file_path: &str, secrets: &Secret) -> std::io::Result<()> {
    let file = File::create(file_path)?;
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, secrets).expect("Failed to write secrets JSON");

    Ok(())
}

fn get_cloudflare_zone_ids(
    api_key: &str,
    domains: &[String],
    logger: &mut Logger,
) -> io::Result<Vec<String>> {
    let zone_result: String = execute(
        Command::new("curl")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", api_key))
            .arg("https://api.cloudflare.com/client/v4/zones/"),
    )?;

    let response: APIResult =
        serde_json::from_str(&zone_result).expect("Expected Zone IDs deserialisation");

    if !response.success {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("There was an error: {}", zone_result),
        ));
    }

    Ok(response
        .result
        .iter()
        .filter_map(|zone| {
            if !domains.contains(&&zone.name) {
                return None;
            }
            logger.log(&format!("{} ({})", zone.name, zone.id));
            Some(zone.id.clone())
        })
        .collect())
}

fn update_cloudflare_zone_ip(
    api_key: &str,
    zone_id: &str,
    new_ip: &str,
    logger: &mut Logger,
) -> io::Result<String> {
    let zone_result: String = execute(
        Command::new("curl")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", api_key))
            .arg(format!(
                "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
                zone_id
            )),
    )?;

    let mut response: APIResult =
        serde_json::from_str(&zone_result).expect("Expected DNS Records deserialisation");

    if !response.success {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("There was an error: {}", zone_result),
        ));
    }

    let mut update = false;

    let mut batch_data = r#"{"patches": ["#.to_string();
    batch_data.push_str(
        &response
            .result
            .iter_mut()
            .filter_map(|zone| {
                if zone.r#type.is_none() || zone.content.is_none() {
                    return None;
                }

                if zone.r#type.as_ref().unwrap() != &"A"
                    || zone.content.as_ref().unwrap() == new_ip
                    || zone.comment.is_some()
                {
                    logger.log(&format!("{} ({}) - Skipping", zone.name, zone.id));
                    return None;
                }

                logger.log(&format!("{} ({})", zone.name, zone.id));

                zone.content = Some(new_ip.to_string());
                update = true;

                Some(serde_json::to_string(&zone).expect("Expect to make API batch string"))
            })
            .collect::<Vec<_>>()
            .join(","),
    );

    if !update {
        return Ok("0".to_string());
    }

    batch_data.push_str("]}");

    let batch_result = execute(
        Command::new("curl")
            .arg("-X")
            .arg("POST")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", api_key))
            .arg("-d")
            .arg(batch_data)
            .arg(format!(
                "https://api.cloudflare.com/client/v4/zones/{}/dns_records/batch",
                zone_id
            )),
    )?;

    if !batch_result.contains(r#""success":true"#) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("There was an error: {}", zone_result),
        ));
    }

    Ok(batch_result)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!(
            "Usage: {} <API_KEY | file:<PATH>> <DOMAIN_1> [DOMAIN_N ...]\n\
             \n\
             Requirements:\n\
             \x20 curl must be installed and available on PATH\n\
             \n\
             Arguments:\n\
             \x20 API_KEY          Cloudflare API key (Bearer token)\n\
             \x20 file:<PATH>      Path to a JSON secrets file (e.g. file:./.secret)\n\
             \n\
             Secrets file fields:\n\
             \x20 cloudflare_api_key  (required) Cloudflare Bearer token\n\
             \x20 last_ip             (optional) Last known public IP; skips update if unchanged\n\
             \x20 log_path            (optional) Path to append log output\n\
             \n\
             Special commands (when using file:<PATH>):\n\
             \x20 {} file:<PATH> DEFAULT   Create a new secrets file with default values",
            args[0], args[0]
        );
        exit(2);
    }

    let api_details = &args[1];

    let mut secrets;
    if api_details.starts_with(FILE_HEADER) {
        let file_path = remove_file_header(api_details);

        if &args[2].to_uppercase() == "DEFAULT" {
            write_secrets(file_path, &Secret::default()).expect("Expected to write Secrets file");
            println!("Created Secrets file: {}", file_path);
            exit(0);
        }

        secrets = match read_secrets(file_path) {
            Err(e) => {
                eprintln!("Error reading Secrets file ({}): {}", api_details, e);
                exit(2);
            }
            Ok(s) => s,
        };
    } else {
        secrets = Secret::new(api_details);
    }

    let mut logger = Logger::open(secrets.log_path.as_deref());

    if secrets.is_file {
        logger.log(&format!(
            "Key (Ending): {}",
            &secrets.cloudflare_api_key[secrets.cloudflare_api_key.len() - 4..]
        ));
        logger.log(&format!(
            "Last Ip: {}",
            secrets.last_ip.as_deref().unwrap_or("N/A")
        ));
    }

    let public_ip = match get_public_ip() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Error curling Ip: {}", e);
            exit(1);
        }
    };

    if secrets.last_ip.is_some() && public_ip == *secrets.last_ip.as_ref().unwrap() {
        logger.log("Ip hasn't changed. No changes made");
        exit(0);
    }

    logger.log(&public_ip);
    logger.log("Getting Zone Ids");

    let zone_ids =
        match get_cloudflare_zone_ids(&secrets.cloudflare_api_key, &args[2..], &mut logger) {
            Err(e) => {
                eprintln!("Error Querying Zones: {}", e);
                exit(1);
            }
            Ok(ids) => ids,
        };

    if zone_ids.len() != args.len() - 2 {
        eprintln!("Unable to find all domains.\n{}", zone_ids.join("\n"));
        exit(1);
    };

    logger.log("Updating Zones");
    for zone_id in zone_ids {
        match update_cloudflare_zone_ip(
            &secrets.cloudflare_api_key,
            &zone_id,
            &public_ip,
            &mut logger,
        ) {
            Err(e) => eprintln!("Error Updating Zone: {}", e),
            Ok(r) => match r.as_str() {
                "0" => logger.log(&format!(
                    "Zone Skipped (ID:{}): No Records to Update",
                    zone_id
                )),
                _ => logger.log(&format!("Zone Updated (ID:{})", zone_id)),
            },
        }
    }

    if secrets.is_file {
        secrets.last_ip = Some(public_ip.to_owned());
        let _ = write_secrets(remove_file_header(api_details), &secrets);
    }

    logger.log("SUCCESS");
}
