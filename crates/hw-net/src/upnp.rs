//! Automatic port mapping (UPnP): asks the home router to forward the game's
//! UDP port to this PC, so friends can join a hosted world by address
//! without the relay, and without anyone opening ports by hand.
//!
//! Std only: SSDP finds the router (a UDP search on 239.255.255.250:1900),
//! its description (plain HTTP) names the WAN connection's control URL, and
//! two SOAP calls ask for the external address and the mapping. The mapping
//! is removed again when the [`PortMapping`] is dropped. Routers without
//! UPnP simply say nothing: the relay still works.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// Where routers listen for searches.
pub const SSDP: &str = "239.255.255.250:1900";
/// How long to wait for a router to answer a search.
const SEARCH_WAIT: Duration = Duration::from_millis(2500);
/// Time limit for each HTTP call to the router.
const HTTP_WAIT: Duration = Duration::from_secs(3);

const SEARCH_FOR: [&str; 3] = [
    "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
    "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
];

/// A port the router forwards to us, until dropped.
#[derive(Debug)]
pub struct PortMapping {
    /// The router's internet address, if it said.
    pub external_ip: Option<String>,
    pub port: u16,
    control: String,
    service: String,
}

impl PortMapping {
    /// Where friends on the internet reach this PC, if the router said.
    pub fn address(&self) -> Option<String> {
        self.external_ip
            .as_ref()
            .map(|ip| format!("{ip}:{}", self.port))
    }
}

impl Drop for PortMapping {
    fn drop(&mut self) {
        let args = format!(
            "<NewRemoteHost></NewRemoteHost><NewExternalPort>{}</NewExternalPort>\
             <NewProtocol>UDP</NewProtocol>",
            self.port
        );
        let _ = soap(&self.control, &self.service, "DeletePortMapping", &args);
    }
}

/// Asks the router to forward UDP `port` to this PC. `ssdp` is where to
/// search ([`SSDP`]; tests use a stand-in).
pub fn map_udp(port: u16, description: &str, ssdp: &str) -> Result<PortMapping, String> {
    let location = discover(ssdp)?;
    let description_xml = http_get(&location)?;
    let (control, service) =
        control_url(&location, &description_xml).ok_or("the router does not offer port mapping")?;
    let local = local_ip(&location)?;
    let args = format!(
        "<NewRemoteHost></NewRemoteHost><NewExternalPort>{port}</NewExternalPort>\
         <NewProtocol>UDP</NewProtocol><NewInternalPort>{port}</NewInternalPort>\
         <NewInternalClient>{local}</NewInternalClient><NewEnabled>1</NewEnabled>\
         <NewPortMappingDescription>{}</NewPortMappingDescription>\
         <NewLeaseDuration>0</NewLeaseDuration>",
        escape(description)
    );
    soap(&control, &service, "AddPortMapping", &args)?;
    let external_ip = soap(&control, &service, "GetExternalIPAddress", "")
        .ok()
        .and_then(|r| tag(&r, "NewExternalIPAddress").map(str::to_string))
        .filter(|ip| ip.parse::<IpAddr>().is_ok());
    Ok(PortMapping {
        external_ip,
        port,
        control,
        service,
    })
}

/// [`map_udp`] on a thread of its own (it can take a few seconds): the
/// answer arrives on the receiver.
pub fn map_udp_in_background(
    port: u16,
    description: &str,
) -> Receiver<Result<PortMapping, String>> {
    let (tx, rx) = channel();
    let description = description.to_string();
    let _ = std::thread::Builder::new()
        .name("upnp".into())
        .spawn(move || {
            let _ = tx.send(map_udp(port, &description, SSDP));
        });
    rx
}

/// Searches for a router; returns its description's URL.
fn discover(ssdp: &str) -> Result<String, String> {
    let to: SocketAddr = ssdp
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or("no search address")?;
    let socket = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    for st in SEARCH_FOR {
        let msg = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nST: {st}\r\n\
             MAN: \"ssdp:discover\"\r\nMX: 2\r\n\r\n"
        );
        socket
            .send_to(msg.as_bytes(), to)
            .map_err(|e| format!("cannot search for the router: {e}"))?;
    }
    let start = Instant::now();
    let mut buf = [0u8; 2048];
    while start.elapsed() < SEARCH_WAIT {
        let Ok((n, _)) = socket.recv_from(&mut buf) else {
            // Nothing yet (or a refusal on a local test address).
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        let text = String::from_utf8_lossy(&buf[..n]);
        if let Some(loc) = header(&text, "location") {
            if loc.starts_with("http://") {
                return Ok(loc.to_string());
            }
        }
    }
    Err("no router answered (UPnP is off or missing)".into())
}

/// A header's value in an HTTP-like message (name in any case).
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// `http://host:port/path` as (host:port, /path).
fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("http://")?;
    Some(match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    })
}

fn http(url: &str, request: impl Fn(&str, &str) -> String) -> Result<String, String> {
    let (authority, path) = split_url(url).ok_or_else(|| format!("not an http URL: {url}"))?;
    let addr = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let to = addr
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or_else(|| format!("cannot find {addr}"))?;
    let mut s = TcpStream::connect_timeout(&to, HTTP_WAIT).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(HTTP_WAIT))
        .map_err(|e| e.to_string())?;
    s.write_all(request(authority, path).as_bytes())
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out).to_string();
    let status = text.lines().next().unwrap_or_default();
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    if status.split_whitespace().nth(1) == Some("200") {
        Ok(body)
    } else {
        // A SOAP fault says why.
        let why = tag(&body, "errorDescription").unwrap_or(status);
        Err(format!("the router said: {why}"))
    }
}

fn http_get(url: &str) -> Result<String, String> {
    http(url, |host, path| {
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
    })
}

/// Calls `action` of the router's WAN connection service.
fn soap(control: &str, service: &str, action: &str, args: &str) -> Result<String, String> {
    let body = format!(
        "<?xml version=\"1.0\"?>\r\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>\
         <u:{action} xmlns:u=\"{service}\">{args}</u:{action}></s:Body></s:Envelope>\r\n"
    );
    http(control, |host, path| {
        format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
             Content-Type: text/xml; charset=\"utf-8\"\r\nContent-Length: {}\r\n\
             SOAPAction: \"{service}#{action}\"\r\n\r\n{body}",
            body.len()
        )
    })
}

/// The text inside the first `<name>` element (any namespace prefix).
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = xml[from..].find('<').map(|i| i + from) {
        let end = xml[i..].find('>')? + i;
        let open = &xml[i + 1..end];
        let bare = open.rsplit(':').next().unwrap_or(open);
        let bare = bare.split_whitespace().next().unwrap_or(bare);
        if bare == name && !open.starts_with('/') {
            let close = xml[end..].find("</")? + end;
            return Some(xml[end + 1..close].trim());
        }
        from = end;
    }
    None
}

/// The control URL and service type of the WAN connection in a router's
/// description, made absolute against where the description came from.
fn control_url(location: &str, xml: &str) -> Option<(String, String)> {
    let (authority, _) = split_url(location)?;
    let base = tag(xml, "URLBase")
        .map(|b| b.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("http://{authority}"));
    for block in xml.split("<service>").skip(1) {
        let block = block.split("</service>").next()?;
        let service = tag(block, "serviceType")?;
        if !(service.contains("WANIPConnection") || service.contains("WANPPPConnection")) {
            continue;
        }
        let control = tag(block, "controlURL")?;
        let url = if control.starts_with("http://") {
            control.to_string()
        } else if control.starts_with('/') {
            format!("{base}{control}")
        } else {
            format!("{base}/{control}")
        };
        return Some((url, service.to_string()));
    }
    None
}

/// This PC's address on the router's network.
fn local_ip(location: &str) -> Result<IpAddr, String> {
    let (authority, _) = split_url(location).ok_or("bad router address")?;
    let addr = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let s = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
    s.connect(addr).map_err(|e| e.to_string())?;
    Ok(s.local_addr().map_err(|e| e.to_string())?.ip())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const DESCRIPTION: &str = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0"><device>
<deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
<deviceList><device><deviceList><device><serviceList>
<service><serviceType>urn:schemas-upnp-org:service:WANCommonInterfaceConfig:1</serviceType>
<controlURL>/ctl/CmnIfCfg</controlURL></service>
<service><serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
<serviceId>urn:upnp-org:serviceId:WANIPConn1</serviceId>
<controlURL>/ctl/IPConn</controlURL></service>
</serviceList></device></deviceList></device></deviceList></device></root>"#;

    /// A stand-in router: answers searches on a UDP port and serves its
    /// description and SOAP calls over HTTP. Returns the search address and
    /// the actions it was asked for.
    fn fake_router() -> (String, Arc<Mutex<Vec<String>>>) {
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let web = http.local_addr().unwrap();
        let ssdp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let search = ssdp.local_addr().unwrap().to_string();
        let calls = Arc::new(Mutex::new(Vec::new()));
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok((n, from)) = ssdp.recv_from(&mut buf) {
                if String::from_utf8_lossy(&buf[..n]).starts_with("M-SEARCH") {
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\n\
                         LOCATION: http://{web}/rootDesc.xml\r\nST: x\r\n\r\n"
                    );
                    let _ = ssdp.send_to(reply.as_bytes(), from);
                }
            }
        });
        let seen = calls.clone();
        std::thread::spawn(move || {
            for s in http.incoming().map_while(Result::ok) {
                let mut s = s;
                s.set_read_timeout(Some(Duration::from_millis(300)))
                    .unwrap();
                let mut buf = vec![0u8; 8192];
                let mut got = Vec::new();
                while let Ok(n) = s.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    got.extend_from_slice(&buf[..n]);
                    let t = String::from_utf8_lossy(&got);
                    let done = t.starts_with("GET") && t.contains("\r\n\r\n")
                        || t.contains("</s:Envelope>");
                    if done {
                        break;
                    }
                }
                let req = String::from_utf8_lossy(&got).to_string();
                let body = if req.starts_with("GET /rootDesc.xml") {
                    DESCRIPTION.to_string()
                } else if let Some(action) = header(&req, "soapaction") {
                    let action = action
                        .trim_matches('"')
                        .rsplit('#')
                        .next()
                        .unwrap()
                        .to_string();
                    seen.lock().unwrap().push(format!(
                        "{action} {}",
                        tag(&req, "NewExternalPort").unwrap_or("")
                    ));
                    match action.as_str() {
                        "GetExternalIPAddress" => "<s:Envelope><s:Body><u:R>\
                            <NewExternalIPAddress>84.12.3.4</NewExternalIPAddress>\
                            </u:R></s:Body></s:Envelope>"
                            .to_string(),
                        _ => "<s:Envelope><s:Body></s:Body></s:Envelope>".to_string(),
                    }
                } else {
                    String::new()
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (search, calls)
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn maps_a_port_on_a_router_and_frees_it_again() {
        let (search, calls) = fake_router();
        let m = map_udp(7777, "HeroWorld", &search).expect("mapped");
        assert_eq!(m.address().as_deref(), Some("84.12.3.4:7777"));
        assert!(m.control.ends_with("/ctl/IPConn"), "{}", m.control);
        drop(m);
        assert_eq!(
            *calls.lock().unwrap(),
            [
                "AddPortMapping 7777",
                "GetExternalIPAddress ",
                "DeletePortMapping 7777"
            ]
        );
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn no_router_says_so() {
        // Nobody listens there.
        let quiet = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = quiet.local_addr().unwrap().to_string();
        drop(quiet);
        assert!(map_udp(7777, "HeroWorld", &addr).is_err());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn reads_router_descriptions() {
        let (url, service) =
            control_url("http://192.168.1.1:5000/rootDesc.xml", DESCRIPTION).unwrap();
        assert_eq!(url, "http://192.168.1.1:5000/ctl/IPConn");
        assert_eq!(service, "urn:schemas-upnp-org:service:WANIPConnection:1");
        assert_eq!(tag("<a><u:B x=\"1\"> v </u:B></a>", "B"), Some("v"));
        assert_eq!(
            header("HTTP/1.1 200 OK\r\nLocation: http://x/y\r\n", "LOCATION"),
            Some("http://x/y")
        );
    }
}
