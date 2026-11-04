use std::io::{Read, Write};
use std::time::Duration;

use std::collections::HashMap;

use drifter_core::settings::{FoundPrinter, PrintJob, Printer};

const SOCKET: &str = "/run/cups/cups.sock";
const PORT: (&str, u16) = ("localhost", 631);
const WAIT: Duration = Duration::from_secs(8);
const TEST_PAGE: &str = "/usr/share/cups/data/testprint";

pub const PRINT_JOB: u16 = 0x0002;
pub const GET_JOBS: u16 = 0x000a;
pub const CUPS_GET_DEFAULT: u16 = 0x4001;
pub const CUPS_GET_PRINTERS: u16 = 0x4002;

pub const OPERATION: u8 = 0x01;
pub const JOB: u8 = 0x02;
pub const END: u8 = 0x03;
pub const PRINTER: u8 = 0x04;

pub const INTEGER: u8 = 0x21;
pub const BOOLEAN: u8 = 0x22;
pub const ENUM: u8 = 0x23;
pub const TEXT_WITH_LANGUAGE: u8 = 0x35;
pub const NAME_WITH_LANGUAGE: u8 = 0x36;
#[cfg(test)]
pub const TEXT: u8 = 0x41;
pub const NAME: u8 = 0x42;
pub const KEYWORD: u8 = 0x44;
pub const URI: u8 = 0x45;
pub const CHARSET: u8 = 0x47;
pub const LANGUAGE: u8 = 0x48;

const PRINTER_ASKED: [&str; 9] = [
    "printer-name",
    "printer-info",
    "printer-location",
    "printer-make-and-model",
    "printer-state",
    "printer-state-message",
    "printer-is-accepting-jobs",
    "printer-uri-supported",
    "device-uri",
];

const JOB_ASKED: [&str; 5] = ["job-id", "job-name", "job-state", "job-printer-uri", "job-originating-user-name"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seen {
    pub printers: Vec<Printer>,
    pub default: Option<String>,
    pub jobs: Vec<PrintJob>,
}

pub struct Request {
    bytes: Vec<u8>,
}

impl Request {
    pub fn new(operation: u16, id: u32) -> Request {
        let mut bytes = vec![2, 0];
        bytes.extend(operation.to_be_bytes());
        bytes.extend(id.to_be_bytes());
        bytes.push(OPERATION);
        let mut request = Request { bytes };
        request.text(CHARSET, "attributes-charset", &["utf-8"]);
        request.text(LANGUAGE, "attributes-natural-language", &["en"]);
        request
    }

    fn value(&mut self, tag: u8, name: &str, value: &[u8]) {
        self.bytes.push(tag);
        self.bytes.extend((name.len() as u16).to_be_bytes());
        self.bytes.extend(name.as_bytes());
        self.bytes.extend((value.len() as u16).to_be_bytes());
        self.bytes.extend(value);
    }

    pub fn text(&mut self, tag: u8, name: &str, values: &[&str]) -> &mut Request {
        for (at, value) in values.iter().enumerate() {
            self.value(tag, if at == 0 { name } else { "" }, value.as_bytes());
        }
        self
    }

    pub fn finish(mut self, document: &[u8]) -> Vec<u8> {
        self.bytes.push(END);
        self.bytes.extend(document);
        self.bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Text(String),
    Other(u8, Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub tag: u8,
    pub attrs: Vec<Attr>,
}

impl Group {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.attrs.iter().find(|attr| attr.name == name)?.values.first()
    }

    pub fn text(&self, name: &str) -> String {
        match self.get(name) {
            Some(Value::Text(text)) => text.clone(),
            _ => String::new(),
        }
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        match self.get(name) {
            Some(Value::Int(number)) => Some(*number),
            _ => None,
        }
    }

    pub fn boolean(&self, name: &str) -> Option<bool> {
        match self.get(name) {
            Some(Value::Bool(on)) => Some(*on),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub id: u32,
    pub groups: Vec<Group>,
}

impl Response {
    pub fn ok(&self) -> bool {
        self.status < 0x0100
    }

    pub fn all(&self, tag: u8) -> impl Iterator<Item = &Group> {
        self.groups.iter().filter(move |group| group.tag == tag)
    }

    pub fn said(&self) -> String {
        self.all(OPERATION)
            .map(|group| group.text("status-message"))
            .find(|said| !said.is_empty())
            .unwrap_or_else(|| format!("IPP status 0x{:04x}", self.status))
    }
}

struct Bytes<'a> {
    rest: &'a [u8],
}

impl<'a> Bytes<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        if self.rest.len() < count {
            return Err("the IPP answer was cut short".to_string());
        }
        let (taken, rest) = self.rest.split_at(count);
        self.rest = rest;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn short(&mut self) -> Result<u16, String> {
        let two = self.take(2)?;
        Ok(u16::from_be_bytes([two[0], two[1]]))
    }

    fn long(&mut self) -> Result<u32, String> {
        let four = self.take(4)?;
        Ok(u32::from_be_bytes([four[0], four[1], four[2], four[3]]))
    }

    fn sized(&mut self) -> Result<&'a [u8], String> {
        let count = usize::from(self.short()?);
        self.take(count)
    }
}

fn read_value(tag: u8, raw: &[u8]) -> Value {
    let words = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    match (tag, raw.len()) {
        (INTEGER | ENUM, 4) => Value::Int(i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])),
        (BOOLEAN, 1) => Value::Bool(raw[0] != 0),
        (TEXT_WITH_LANGUAGE | NAME_WITH_LANGUAGE, _) => {
            let mut bytes = Bytes { rest: raw };
            let text = bytes.sized().and_then(|_| bytes.sized());
            match text {
                Ok(text) => Value::Text(words(text)),
                Err(_) => Value::Other(tag, raw.to_vec()),
            }
        }
        (0x40..=0x49, _) => Value::Text(words(raw)),
        _ => Value::Other(tag, raw.to_vec()),
    }
}

pub fn decode(bytes: &[u8]) -> Result<Response, String> {
    let mut bytes = Bytes { rest: bytes };
    let _version = bytes.short()?;
    let status = bytes.short()?;
    let id = bytes.long()?;
    let mut groups: Vec<Group> = Vec::new();
    loop {
        let tag = bytes.byte()?;
        if tag == END {
            break;
        }
        if tag < 0x10 {
            groups.push(Group { tag, attrs: Vec::new() });
            continue;
        }
        let name = bytes.sized()?;
        let raw = bytes.sized()?;
        let value = read_value(tag, raw);
        let Some(group) = groups.last_mut() else {
            return Err("the IPP answer has a value outside any group".to_string());
        };
        match (name.is_empty(), group.attrs.last_mut()) {
            (true, Some(attr)) => attr.values.push(value),
            (true, None) => return Err("the IPP answer has a value with no name".to_string()),
            (false, _) => group.attrs.push(Attr {
                name: String::from_utf8_lossy(name).into_owned(),
                values: vec![value],
            }),
        }
    }
    Ok(Response { status, id, groups })
}

fn http_request(path: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend(body);
    out
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let end = body
            .windows(2)
            .position(|pair| pair == b"\r\n")
            .ok_or("a chunk of the CUPS answer has no size")?;
        let size = String::from_utf8_lossy(&body[..end]);
        let size = size.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size, 16).map_err(|_| "a chunk of the CUPS answer has a bad size")?;
        body = &body[end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size {
            return Err("the CUPS answer was cut short".to_string());
        }
        out.extend(&body[..size]);
        body = body.get(size + 2..).unwrap_or(&[]);
    }
}

pub fn http_body(answer: &[u8]) -> Result<Vec<u8>, String> {
    let split = answer
        .windows(4)
        .position(|four| four == b"\r\n\r\n")
        .ok_or("CUPS sent no HTTP header")?;
    let head = String::from_utf8_lossy(&answer[..split]).to_ascii_lowercase();
    let body = &answer[split + 4..];
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        let line = head.lines().next().unwrap_or("").to_string();
        return Err(format!("CUPS answered {line}"));
    }
    let header = |name: &str| {
        head.lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim().to_string())
    };
    if header("transfer-encoding").is_some_and(|value| value.contains("chunked")) {
        return dechunk(body);
    }
    match header("content-length").and_then(|value| value.parse::<usize>().ok()) {
        Some(length) if body.len() >= length => Ok(body[..length].to_vec()),
        Some(_) => Err("the CUPS answer was cut short".to_string()),
        None => Ok(body.to_vec()),
    }
}

fn talk(mut stream: impl Read + Write, request: &[u8]) -> std::io::Result<Vec<u8>> {
    stream.write_all(request)?;
    stream.flush()?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer)?;
    Ok(answer)
}

fn send(path: &str, body: &[u8]) -> Result<Response, String> {
    let request = http_request(path, body);
    let answer = match std::os::unix::net::UnixStream::connect(SOCKET) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(WAIT));
            let _ = stream.set_write_timeout(Some(WAIT));
            talk(stream, &request)
        }
        Err(_) => std::net::TcpStream::connect(PORT).and_then(|stream| {
            let _ = stream.set_read_timeout(Some(WAIT));
            let _ = stream.set_write_timeout(Some(WAIT));
            talk(stream, &request)
        }),
    }
    .map_err(|error| error.to_string())?;
    decode(&http_body(&answer)?)
}

pub fn running() -> bool {
    std::os::unix::net::UnixStream::connect(SOCKET).is_ok()
        || std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], PORT.1)), Duration::from_millis(500)).is_ok()
}

pub fn user() -> String {
    ["USER", "LOGNAME"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|name| !name.is_empty())
        .unwrap_or_else(|| "drifter".to_string())
}

fn printer_uri(name: &str) -> String {
    let safe: String = name
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect();
    format!("ipp://localhost/printers/{safe}")
}

pub fn printers_request() -> Vec<u8> {
    let mut request = Request::new(CUPS_GET_PRINTERS, 1);
    request.text(NAME, "requesting-user-name", &[&user()]).text(KEYWORD, "requested-attributes", &PRINTER_ASKED);
    request.finish(&[])
}

pub fn default_request() -> Vec<u8> {
    let mut request = Request::new(CUPS_GET_DEFAULT, 2);
    request.text(NAME, "requesting-user-name", &[&user()]).text(KEYWORD, "requested-attributes", &["printer-name"]);
    request.finish(&[])
}

pub fn jobs_request() -> Vec<u8> {
    let mut request = Request::new(GET_JOBS, 3);
    request
        .text(URI, "printer-uri", &["ipp://localhost/"])
        .text(NAME, "requesting-user-name", &[&user()])
        .text(KEYWORD, "which-jobs", &["not-completed"])
        .text(KEYWORD, "requested-attributes", &JOB_ASKED);
    request.finish(&[])
}

pub fn test_page_request(printer: &str, page: &[u8]) -> Vec<u8> {
    let mut request = Request::new(PRINT_JOB, 4);
    request
        .text(URI, "printer-uri", &[&printer_uri(printer)])
        .text(NAME, "requesting-user-name", &[&user()])
        .text(NAME, "job-name", &["Test Page"]);
    request.finish(page)
}

pub fn printers_of(response: &Response) -> Vec<Printer> {
    let mut printers: Vec<Printer> = response
        .all(PRINTER)
        .filter(|group| !group.text("printer-name").is_empty())
        .map(|group| Printer {
            name: group.text("printer-name"),
            info: group.text("printer-info"),
            location: group.text("printer-location"),
            model: group.text("printer-make-and-model"),
            state: group.int("printer-state").unwrap_or(3) as u32,
            message: group.text("printer-state-message"),
            accepting: group.boolean("printer-is-accepting-jobs").unwrap_or(true),
            uri: group.text("printer-uri-supported"),
            device: group.text("device-uri"),
        })
        .collect();
    printers.sort_by_key(|printer| printer.shown().to_lowercase());
    printers
}

pub fn jobs_of(response: &Response) -> Vec<PrintJob> {
    response
        .all(JOB)
        .filter_map(|group| {
            let uri = group.text("job-printer-uri");
            Some(PrintJob {
                id: u32::try_from(group.int("job-id")?).ok()?,
                name: group.text("job-name"),
                printer: uri.rsplit('/').next().unwrap_or("").to_string(),
                state: group.int("job-state").unwrap_or(3) as u32,
                user: group.text("job-originating-user-name"),
            })
        })
        .collect()
}

pub fn default_of(response: &Response) -> Option<String> {
    if !response.ok() {
        return None;
    }
    let name = response.all(PRINTER).map(|group| group.text("printer-name")).find(|name| !name.is_empty())?;
    Some(name)
}

pub fn read() -> Result<Seen, String> {
    let printers = send("/", &printers_request())?;
    if !printers.ok() && printers.status != 0x0406 {
        return Err(printers.said());
    }
    let default = send("/", &default_request()).ok().and_then(|response| default_of(&response));
    let jobs = send("/", &jobs_request()).map(|response| jobs_of(&response)).unwrap_or_default();
    Ok(Seen {
        printers: printers_of(&printers),
        default,
        jobs,
    })
}

pub fn print_test_page(printer: &str) -> Result<(), String> {
    let page = std::fs::read(TEST_PAGE).map_err(|error| format!("{TEST_PAGE}: {error}"))?;
    let path = format!("/printers/{}", printer_uri(printer).rsplit('/').next().unwrap_or(""));
    let response = send(&path, &test_page_request(printer, &page))?;
    match response.ok() {
        true => Ok(()),
        false => Err(response.said()),
    }
}

const HELPER: &str = "org.opensuse.CupsPkHelper.Mechanism";
const FIND_SECONDS: i32 = 10;

pub fn helper_ready() -> bool {
    let Ok(bus) = zbus::blocking::Connection::system() else {
        return false;
    };
    let names: Result<Vec<String>, _> = bus
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "ListActivatableNames",
            &(),
        )
        .and_then(|reply| reply.body().deserialize());
    names.is_ok_and(|names| names.iter().any(|name| name == HELPER))
}

fn helper<B, R>(method: &str, body: &B) -> Result<R, String>
where
    B: zbus::export::serde::ser::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let bus = zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.method_timeout(Duration::from_secs(300)).build())
        .map_err(|error| format!("system bus: {error}"))?;
    let proxy = zbus::blocking::Proxy::new(&bus, HELPER, "/", HELPER).map_err(|error| error.to_string())?;
    let reply: Option<R> = proxy
        .call_with_flags(method, zbus::proxy::MethodFlags::AllowInteractiveAuth.into(), body)
        .map_err(|error| match error {
            zbus::Error::MethodError(name, _, _) if name.as_str().ends_with("ServiceUnknown") => {
                drifter_core::lang::t("Install the cups-pk-helper package to add or change printers.").to_string()
            }
            zbus::Error::MethodError(name, said, _)
                if name.as_str().ends_with("NotPrivileged") || said.as_deref().is_some_and(|said| said.starts_with("Not Authorized")) =>
            {
                drifter_core::lang::t("Not changed. This needs an administrator password.").to_string()
            }
            zbus::Error::MethodError(_, Some(said), _) => said,
            other => other.to_string(),
        })?;
    reply.ok_or_else(|| "cups-pk-helper sent no answer".to_string())
}

fn said(error: String) -> Result<(), String> {
    match error.trim().is_empty() {
        true => Ok(()),
        false => Err(error.trim().to_string()),
    }
}

pub fn set_default(name: &str) -> Result<(), String> {
    said(helper("PrinterSetDefault", &(name,))?)
}

pub fn set_enabled(name: &str, on: bool) -> Result<(), String> {
    said(helper("PrinterSetEnabled", &(name, on))?)?;
    match on {
        true => said(helper("PrinterSetAcceptJobs", &(name, true, ""))?),
        false => Ok(()),
    }
}

pub fn remove(name: &str) -> Result<(), String> {
    said(helper("PrinterDelete", &(name,))?)
}

pub fn cancel(job: u32) -> Result<(), String> {
    let id = i32::try_from(job).map_err(|_| "bad job number".to_string())?;
    said(helper("JobCancelPurge", &(id, false))?)
}

pub fn find() -> Result<Vec<FoundPrinter>, String> {
    let none: Vec<&str> = Vec::new();
    let (error, devices): (String, HashMap<String, String>) = helper("DevicesGet", &(FIND_SECONDS, 0i32, none.clone(), none))?;
    if !error.trim().is_empty() {
        return Err(error.trim().to_string());
    }
    Ok(devices_of(&devices))
}

pub fn devices_of(devices: &HashMap<String, String>) -> Vec<FoundPrinter> {
    let mut numbers: Vec<usize> = devices
        .keys()
        .filter_map(|key| key.strip_prefix("device-uri:")?.parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers
        .into_iter()
        .map(|at| {
            let field = |name: &str| devices.get(&format!("{name}:{at}")).cloned().unwrap_or_default();
            FoundPrinter {
                uri: field("device-uri"),
                info: field("device-info"),
                model: field("device-make-and-model"),
            }
        })
        .collect()
}

pub fn add(found: &FoundPrinter, taken: &[String]) -> Result<String, String> {
    let base = found.queue();
    let name = (1..)
        .map(|n| match n {
            1 => base.clone(),
            n => format!("{base}_{n}"),
        })
        .find(|name| !taken.iter().any(|each| each.eq_ignore_ascii_case(name)))
        .unwrap_or(base);
    let info = found.shown();
    said(helper("PrinterAdd", &(name.as_str(), found.uri.as_str(), "everywhere", info.as_str(), ""))?)?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr(out: &mut Vec<u8>, tag: u8, name: &str, value: &[u8]) {
        out.push(tag);
        out.extend((name.len() as u16).to_be_bytes());
        out.extend(name.as_bytes());
        out.extend((value.len() as u16).to_be_bytes());
        out.extend(value);
    }

    fn head(status: u16, id: u32) -> Vec<u8> {
        let mut out = vec![2, 0];
        out.extend(status.to_be_bytes());
        out.extend(id.to_be_bytes());
        out.push(OPERATION);
        attr(&mut out, CHARSET, "attributes-charset", b"utf-8");
        attr(&mut out, LANGUAGE, "attributes-natural-language", b"en");
        out
    }

    #[test]
    fn a_request_is_encoded_byte_for_byte() {
        let mut request = Request::new(CUPS_GET_DEFAULT, 7);
        request.text(KEYWORD, "requested-attributes", &["printer-name", "printer-state"]);
        let bytes = request.finish(b"%!");
        let mut want: Vec<u8> = vec![0x02, 0x00, 0x40, 0x01, 0x00, 0x00, 0x00, 0x07, 0x01];
        want.extend([0x47, 0x00, 0x12]);
        want.extend(b"attributes-charset");
        want.extend([0x00, 0x05]);
        want.extend(b"utf-8");
        want.extend([0x48, 0x00, 0x1b]);
        want.extend(b"attributes-natural-language");
        want.extend([0x00, 0x02]);
        want.extend(b"en");
        want.extend([0x44, 0x00, 0x14]);
        want.extend(b"requested-attributes");
        want.extend([0x00, 0x0c]);
        want.extend(b"printer-name");
        want.extend([0x44, 0x00, 0x00, 0x00, 0x0d]);
        want.extend(b"printer-state");
        want.extend([0x03]);
        want.extend(b"%!");
        assert_eq!(bytes, want);
    }

    #[test]
    fn the_jobs_request_asks_for_the_jobs_not_done() {
        let bytes = jobs_request();
        assert_eq!(&bytes[..4], &[0x02, 0x00, 0x00, 0x0a]);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("which-jobs") && text.contains("not-completed"));
        assert_eq!(bytes.last(), Some(&END));
    }

    #[test]
    fn printers_are_read_from_their_groups() {
        let mut bytes = head(0x0000, 1);
        bytes.push(PRINTER);
        attr(&mut bytes, NAME, "printer-name", b"Office_Laser");
        attr(&mut bytes, TEXT, "printer-info", b"Office Laser");
        attr(&mut bytes, TEXT, "printer-location", b"Front desk");
        attr(&mut bytes, TEXT, "printer-make-and-model", b"Drifter Office Laser - IPP Everywhere");
        attr(&mut bytes, ENUM, "printer-state", &5i32.to_be_bytes());
        attr(&mut bytes, TEXT, "printer-state-message", b"Paused by you");
        attr(&mut bytes, BOOLEAN, "printer-is-accepting-jobs", &[1]);
        attr(&mut bytes, URI, "printer-uri-supported", b"ipp://localhost/printers/Office_Laser");
        attr(&mut bytes, URI, "", b"ipps://localhost/printers/Office_Laser");
        bytes.push(PRINTER);
        attr(&mut bytes, NAME, "printer-name", b"Attic");
        attr(&mut bytes, ENUM, "printer-state", &4i32.to_be_bytes());
        attr(&mut bytes, BOOLEAN, "printer-is-accepting-jobs", &[0]);
        bytes.push(END);
        let response = decode(&bytes).unwrap();
        assert!(response.ok());
        let found = response.all(PRINTER).next().unwrap();
        let uris = &found.attrs.iter().find(|attr| attr.name == "printer-uri-supported").unwrap().values;
        assert_eq!(uris.len(), 2, "a nameless value joins the one before it");
        let printers = printers_of(&response);
        assert_eq!(printers.len(), 2);
        assert_eq!(printers[0].name, "Attic", "sorted by the shown name");
        assert_eq!(printers[0].state, 4);
        assert!(!printers[0].accepting);
        let office = &printers[1];
        assert_eq!(office.shown(), "Office Laser");
        assert_eq!(office.location, "Front desk");
        assert_eq!(office.state, 5);
        assert_eq!(office.message, "Paused by you");
        assert_eq!(office.uri, "ipp://localhost/printers/Office_Laser");
    }

    #[test]
    fn jobs_name_their_printer_from_its_uri() {
        let mut bytes = head(0x0000, 3);
        bytes.push(JOB);
        attr(&mut bytes, INTEGER, "job-id", &12i32.to_be_bytes());
        attr(&mut bytes, NAME, "job-name", b"Letter");
        attr(&mut bytes, ENUM, "job-state", &5i32.to_be_bytes());
        attr(&mut bytes, URI, "job-printer-uri", b"ipp://trixie/printers/Office_Laser");
        attr(&mut bytes, NAME, "job-originating-user-name", b"ada");
        bytes.push(JOB);
        attr(&mut bytes, INTEGER, "job-id", &13i32.to_be_bytes());
        let mut with_language = Vec::new();
        with_language.extend(2u16.to_be_bytes());
        with_language.extend(b"en");
        with_language.extend(6u16.to_be_bytes());
        with_language.extend(b"Report");
        attr(&mut bytes, NAME_WITH_LANGUAGE, "job-name", &with_language);
        attr(&mut bytes, ENUM, "job-state", &3i32.to_be_bytes());
        bytes.push(END);
        let jobs = jobs_of(&decode(&bytes).unwrap());
        assert_eq!(jobs.len(), 2);
        assert_eq!((jobs[0].id, jobs[0].printer.as_str(), jobs[0].state), (12, "Office_Laser", 5));
        assert_eq!(jobs[0].user, "ada");
        assert_eq!(jobs[1].name, "Report");
        assert_eq!(jobs[1].printer, "");
    }

    #[test]
    fn no_default_is_a_not_found_answer() {
        let mut bytes = head(0x0406, 2);
        attr(&mut bytes, TEXT, "status-message", b"No default printer.");
        bytes.push(END);
        let response = decode(&bytes).unwrap();
        assert!(!response.ok());
        assert_eq!(response.said(), "No default printer.");
        assert_eq!(default_of(&response), None);
        let mut bytes = head(0x0000, 2);
        bytes.push(PRINTER);
        attr(&mut bytes, NAME, "printer-name", b"Office_Laser");
        bytes.push(END);
        assert_eq!(default_of(&decode(&bytes).unwrap()).as_deref(), Some("Office_Laser"));
    }

    #[test]
    fn a_cut_answer_is_an_error_not_a_panic() {
        let mut bytes = head(0x0000, 1);
        bytes.push(PRINTER);
        attr(&mut bytes, NAME, "printer-name", b"Office_Laser");
        for cut in 0..bytes.len() {
            assert!(decode(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        assert!(decode(&[2, 0, 0, 0, 0, 0, 0, 1, 0x45, 0, 1, b'x', 0, 0, 3]).is_err(), "a value outside any group");
    }

    #[test]
    fn collections_and_odd_values_are_kept_but_do_not_break_the_rest() {
        let mut bytes = head(0x0000, 1);
        bytes.push(PRINTER);
        attr(&mut bytes, 0x34, "media-col-default", b"");
        attr(&mut bytes, 0x4a, "", b"media-size");
        attr(&mut bytes, 0x34, "", b"");
        attr(&mut bytes, 0x37, "", b"");
        attr(&mut bytes, 0x37, "", b"");
        attr(&mut bytes, NAME, "printer-name", b"Lab");
        attr(&mut bytes, 0x31, "printer-current-time", &[7, 234, 11, 3, 18, 0, 0, 0, b'+', 6, 0]);
        bytes.push(END);
        let response = decode(&bytes).unwrap();
        assert_eq!(printers_of(&response)[0].name, "Lab");
    }

    #[test]
    fn the_http_body_is_found_plain_or_chunked() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: 3\r\n\r\nabcdef";
        assert_eq!(http_body(plain).unwrap(), b"abc");
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nwiki\r\n5;x=y\r\npedia\r\n0\r\n\r\n";
        assert_eq!(http_body(chunked).unwrap(), b"wikipedia");
        assert!(http_body(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n").is_err());
        assert!(http_body(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc").is_err());
    }

    #[test]
    fn a_printer_name_is_safe_in_its_uri() {
        assert_eq!(printer_uri("Office_Laser"), "ipp://localhost/printers/Office_Laser");
        assert_eq!(printer_uri("a b/c"), "ipp://localhost/printers/a%20b%2Fc");
    }

    #[test]
    fn found_devices_are_read_in_order() {
        let mut devices = HashMap::new();
        for (key, value) in [
            ("device-uri:1", "ipps://Lab%20Inkjet._ipps._tcp.local/"),
            ("device-info:1", "Drifter Lab Inkjet (driverless)"),
            ("device-make-and-model:1", "Drifter Lab Inkjet"),
            ("device-class:1", "network"),
            ("device-uri:0", "beh"),
            ("device-info:0", "Backend Error Handler"),
            ("device-make-and-model:0", "Unknown"),
        ] {
            devices.insert(key.to_string(), value.to_string());
        }
        let found = devices_of(&devices);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].uri, "beh");
        assert_eq!(found[1].model, "Drifter Lab Inkjet");
        let kept = drifter_core::settings::Printers::addable(found, &[]);
        assert_eq!(kept.len(), 1, "bare backends are not printers");
        assert_eq!(kept[0].queue(), "Drifter_Lab_Inkjet");
    }
}
