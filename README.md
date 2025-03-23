Cloudflare DNS Updater
----------------------
A simple Rust CLI tool that updates your Cloudflare DNS 'A' records with your current public IP. It uses curl to fetch your IP and updates the DNS records via the Cloudflare API only if your IP has changed.

Features
--------
- Tiny Build: Leverages the system's curl, keeping dependencies minimal (std + serde_json).
- IP Caching: Retrieves your public IP from ipify and caches it locally in a JSON file.
- Cloudflare API: Fetches zone IDs and updates DNS records in a batch.
- Zone Filtering: Only updates zones given as domain names as arguments.
- Record Filtering: Only updates 'A' records.
- Secrets Management: Supports loading your Cloudflare API key from the JSON file.
- Error Handling: Provides clear error messages and exits with non-zero codes on failures.

Prerequisites
-------------
- Rust & Cargo (install via rustup)
- curl (must be installed)
- Cloudflare API Key (with proper permissions)

Installation
------------
1. Clone the repository:  
   ```bash
   git clone https://github.com/Andrew-McCall/rs-cloudflare-sync.git
   cd rs-cloudflare-sync
   ```  
3. Build the project:  
   `cargo build --release`  
   (Executable is in target/release)

Usage
-----
Run the tool as follows:  
   `./cloudflare-dns-updater <API_KEY_OR_FILE> <DOMAIN_1> [DOMAIN_2 ...]`  

- API_KEY_OR_FILE: Either your Cloudflare API key or a JSON file path prefixed with "file:" (e.g., file:/path/to/secrets.json).
- Domain Arguments: One or more domains to update.

Examples:
- Using an API key directly:  
   `./cloudflare-dns-updater YOUR_CLOUDFLARE_API_KEY example.com`  
  (This does NOT cache the ip and therefore always updates cloudflare)
- Using a secrets file:  
   `./cloudflare-dns-updater file:/path/to/secrets.json example.com`   

Secrets File Format
-------------------
If using a file, your JSON should look like:  
```json
{
  "cloudflare_api_key": "YOUR_CLOUDFLARE_API_KEY",
  "last_ip": null
}
```  
The last_ip field is updated automatically after each run.

If you pass "DEFAULT" (case-insensitive) as the first domain argument, the tool will create a default secrets file and then exit. For example:  
   `./cloudflare-dns-updater file:/path/to/secrets.json DEFAULT`  

This is useful if you're setting up for the first time.

Contributing & License
----------------------
MIT License.
