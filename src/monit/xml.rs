//! Serde model of the status document Monit posts to its M/Monit collector
//! (`status_xml()` in monit's `src/http/xml.c`, document version 2).
//!
//! Every field is optional: Monit omits whole sections depending on the
//! service type, the platform and whether data has been collected yet.

use serde::{Deserialize, Deserializer};

/// A lenient number: Monit prints values with `printf`, which may yield `nan`,
/// `-nan` or `inf`. Those (and anything unparsable) become `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Num(pub Option<f64>);

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Num(s.trim().parse::<f64>().ok().filter(|v| v.is_finite())))
    }
}

impl Num {
    pub fn i(self) -> Option<i64> {
        self.0.map(|v| v as i64)
    }
}

pub fn num(n: &Option<Num>) -> Option<f64> {
    n.and_then(|n| n.0)
}

pub fn int(n: &Option<Num>) -> Option<i64> {
    n.and_then(|n| n.i())
}

#[derive(Debug, Deserialize)]
pub struct Monit {
    #[serde(rename = "@id")]
    pub id: Option<String>,
    #[serde(rename = "@incarnation")]
    pub incarnation: Option<Num>,
    #[serde(rename = "@version")]
    pub version: Option<String>,
    pub server: Server,
    pub platform: Option<Platform>,
    pub hostgroups: Option<HostGroups>,
    pub services: Option<Services>,
    pub servicegroups: Option<ServiceGroups>,
    pub event: Option<Event>,
}

#[derive(Debug, Deserialize)]
pub struct Server {
    // Version 1 documents carry these as elements instead of attributes.
    pub id: Option<String>,
    pub incarnation: Option<Num>,
    pub version: Option<String>,
    pub uptime: Option<Num>,
    pub poll: Option<Num>,
    pub startdelay: Option<Num>,
    pub localhostname: Option<String>,
    pub controlfile: Option<String>,
    pub httpd: Option<Httpd>,
    pub credentials: Option<Credentials>,
}

#[derive(Debug, Deserialize)]
pub struct Httpd {
    pub address: Option<String>,
    pub port: Option<Num>,
    pub ssl: Option<Num>,
    pub unixsocket: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Platform {
    pub name: Option<String>,
    pub release: Option<String>,
    pub version: Option<String>,
    pub machine: Option<String>,
    pub cpu: Option<Num>,
    pub memory: Option<Num>,
    pub swap: Option<Num>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HostGroups {
    #[serde(rename = "name", default)]
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Services {
    #[serde(rename = "service", default)]
    pub services: Vec<Service>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ServiceGroups {
    #[serde(rename = "servicegroup", default)]
    pub groups: Vec<ServiceGroup>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceGroup {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "service", default)]
    pub services: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Event {
    pub collected_sec: Option<Num>,
    pub collected_usec: Option<Num>,
    pub service: Option<String>,
    #[serde(rename = "type")]
    pub service_type: Option<Num>,
    pub id: Option<Num>,
    pub state: Option<Num>,
    pub action: Option<Num>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Service {
    // Version 2 uses an attribute, version 1 an element.
    #[serde(rename = "@name")]
    pub name_attr: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "@type")]
    pub type_attr: Option<Num>,
    #[serde(rename = "type")]
    pub service_type: Option<Num>,
    pub collected_sec: Option<Num>,
    pub collected_usec: Option<Num>,
    pub status: Option<Num>,
    pub status_hint: Option<Num>,
    pub monitor: Option<Num>,
    pub monitormode: Option<Num>,
    pub onreboot: Option<Num>,
    pub pendingaction: Option<Num>,
    pub every: Option<Every>,

    // system + process
    pub uptime: Option<Num>,
    pub boottime: Option<Num>,
    pub filedescriptors: Option<FileDescriptors>,

    // file, directory, fifo, filesystem, process
    pub mode: Option<String>,
    pub uid: Option<Num>,
    pub euid: Option<Num>,
    pub gid: Option<Num>,
    pub timestamps: Option<Timestamps>,
    pub size: Option<Num>,
    pub hardlink: Option<Num>,
    pub checksum: Option<Checksum>,

    // filesystem
    pub fstype: Option<String>,
    pub fsflags: Option<String>,
    pub block: Option<Usage>,
    pub inode: Option<Usage>,
    pub servicetime: Option<ServiceTime>,

    // filesystem + process
    pub read: Option<IoStats>,
    pub write: Option<IoStats>,

    // net
    pub link: Option<Link>,

    // process
    pub pid: Option<Num>,
    pub ppid: Option<Num>,
    pub threads: Option<Num>,
    pub children: Option<Num>,
    pub memory: Option<Memory>,
    pub cpu: Option<Cpu>,

    // connection tests (any type)
    #[serde(default)]
    pub icmp: Vec<Icmp>,
    #[serde(default)]
    pub port: Vec<Port>,
    #[serde(default)]
    pub unix: Vec<Unix>,

    // system
    pub system: Option<SystemStats>,

    // program
    pub program: Option<Program>,
}

impl Service {
    pub fn name(&self) -> String {
        self.name_attr
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_default()
    }

    pub fn type_id(&self) -> i64 {
        int(&self.service_type)
            .or(int(&self.type_attr))
            .unwrap_or(-1)
    }
}

#[derive(Debug, Deserialize)]
pub struct Every {
    #[serde(rename = "type")]
    pub kind: Option<Num>,
    pub number: Option<Num>,
    pub cron: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileDescriptors {
    // system
    pub allocated: Option<Num>,
    pub unused: Option<Num>,
    pub maximum: Option<Num>,
    // process
    pub open: Option<Num>,
    pub opentotal: Option<Num>,
    pub limit: Option<FdLimit>,
}

#[derive(Debug, Deserialize)]
pub struct FdLimit {
    pub soft: Option<Num>,
    pub hard: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Timestamps {
    pub access: Option<Num>,
    pub change: Option<Num>,
    pub modify: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Checksum {
    #[serde(rename = "@type")]
    pub kind: Option<String>,
    #[serde(rename = "$text")]
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub percent: Option<Num>,
    pub usage: Option<Num>,
    pub total: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceTime {
    pub read: Option<Num>,
    pub write: Option<Num>,
    pub wait: Option<Num>,
    pub run: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct IoStats {
    pub bytesgeneric: Option<IoCounter>,
    pub bytes: Option<IoCounter>,
    pub operations: Option<IoCounter>,
}

#[derive(Debug, Deserialize)]
pub struct IoCounter {
    pub count: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Link {
    pub state: Option<Num>,
    pub speed: Option<Num>,
    pub duplex: Option<Num>,
    pub download: Option<Direction>,
    pub upload: Option<Direction>,
}

#[derive(Debug, Deserialize)]
pub struct Direction {
    pub packets: Option<NowTotal>,
    pub bytes: Option<NowTotal>,
    pub errors: Option<NowTotal>,
}

#[derive(Debug, Deserialize)]
pub struct NowTotal {
    pub now: Option<Num>,
    pub total: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Memory {
    pub percent: Option<Num>,
    pub percenttotal: Option<Num>,
    pub kilobyte: Option<Num>,
    pub kilobytetotal: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Cpu {
    pub percent: Option<Num>,
    pub percenttotal: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Icmp {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub responsetime: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Port {
    pub hostname: Option<String>,
    pub portnumber: Option<Num>,
    pub request: Option<String>,
    pub protocol: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub responsetime: Option<Num>,
    pub certificate: Option<Certificate>,
}

#[derive(Debug, Deserialize)]
pub struct Certificate {
    pub valid: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Unix {
    pub path: Option<String>,
    pub protocol: Option<String>,
    pub responsetime: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct SystemStats {
    pub load: Option<Load>,
    pub cpu: Option<SystemCpu>,
    pub memory: Option<Memory>,
    pub swap: Option<Memory>,
}

#[derive(Debug, Deserialize)]
pub struct Load {
    pub avg01: Option<Num>,
    pub avg05: Option<Num>,
    pub avg15: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct SystemCpu {
    pub user: Option<Num>,
    pub system: Option<Num>,
    pub nice: Option<Num>,
    pub wait: Option<Num>,
    pub hardirq: Option<Num>,
    pub softirq: Option<Num>,
    pub steal: Option<Num>,
    pub guest: Option<Num>,
    pub guestnice: Option<Num>,
}

#[derive(Debug, Deserialize)]
pub struct Program {
    pub started: Option<Num>,
    pub status: Option<Num>,
    pub output: Option<String>,
}

/// Decode the request body. Monit declares ISO-8859-1 but actually emits raw
/// bytes from service names and program output, which in practice is UTF-8.
pub fn decode(body: &[u8]) -> String {
    match std::str::from_utf8(body) {
        Ok(s) => s.to_owned(),
        Err(_) => body.iter().map(|&b| b as char).collect(),
    }
}

pub fn parse(body: &[u8]) -> Result<Monit, quick_xml::DeError> {
    let text = decode(body);
    quick_xml::de::from_str(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE: &str = include_str!("../../tests/fixtures/status.xml");

    #[test]
    fn parses_sample() {
        let m = parse(SAMPLE.as_bytes()).expect("parse");
        assert_eq!(m.server.localhostname.as_deref(), Some("testhost"));
        let services = m.services.unwrap().services;
        assert!(services.len() >= 5);
        let sys = services.iter().find(|s| s.type_id() == 5).unwrap();
        assert!(sys.system.as_ref().unwrap().load.is_some());
        let prog = services.iter().find(|s| s.type_id() == 7).unwrap();
        assert!(
            prog.program
                .as_ref()
                .unwrap()
                .output
                .as_ref()
                .unwrap()
                .contains("🚨")
        );
        let ev = m.event.unwrap();
        assert_eq!(ev.service.as_deref(), Some("nginx"));
    }

    #[test]
    fn lenient_numbers() {
        let m: Memory = quick_xml::de::from_str(
            "<memory><percent>-nan</percent><kilobyte>12</kilobyte></memory>",
        )
        .unwrap();
        assert_eq!(m.percent.unwrap().0, None);
        assert_eq!(m.kilobyte.unwrap().0, Some(12.0));
    }
}
