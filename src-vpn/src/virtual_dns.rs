use hickory_proto::{
  op::{MessageType, OpCode, ResponseCode},
  rr::{DNSClass, RecordType},
};
use std::{collections::HashMap, net::IpAddr};
use tproxy_config::IpCidr;

/// Mappings are stable for the lifetime of a VPN session. Applications may cache
/// addresses beyond DNS TTL; recycling them can silently connect to another host.
pub struct VirtualDns {
  names: HashMap<String, IpAddr>,
  addresses: HashMap<IpAddr, String>,
  next: u128,
  last: u128,
  ipv4: bool,
}
impl VirtualDns {
  pub fn new(pool: IpCidr) -> Self {
    let first = pool.first_address();
    let last = pool.last_address();
    Self {
      names: HashMap::new(),
      addresses: HashMap::new(),
      ipv4: first.is_ipv4(),
      next: match first {
        IpAddr::V4(v) => u32::from(v) as u128,
        IpAddr::V6(v) => u128::from(v),
      } + 1,
      last: match last {
        IpAddr::V4(v) => u32::from(v) as u128,
        IpAddr::V6(v) => u128::from(v),
      },
    }
  }
  pub fn is_fake_ip(ip: &IpAddr) -> bool {
    matches!(ip,IpAddr::V4(v) if v.octets()[0]==198 && (v.octets()[1]==18 || v.octets()[1]==19))
  }
  pub fn resolve_ip(&self, ip: &IpAddr) -> Option<&String> {
    self.addresses.get(ip)
  }
  pub fn generate_query(
    &mut self,
    data: &[u8],
  ) -> anyhow::Result<(Vec<u8>, String, Option<IpAddr>)> {
    let mut request = crate::dns::parse_data_to_dns_message(data, false)?;
    anyhow::ensure!(
      request.metadata.message_type == MessageType::Query,
      "expected DNS query"
    );
    let name = crate::dns::extract_domain_from_dns_message(&request).unwrap_or_default();
    request.answers.clear();
    request.authorities.clear();
    request.additionals.clear();
    request.metadata.message_type = MessageType::Response;
    request.metadata.recursion_available = true;
    request.metadata.authoritative = false;
    request.metadata.authentic_data = false;
    request.metadata.truncation = false;
    request.metadata.response_code = ResponseCode::NoError;
    if request.queries.len() != 1 {
      request.metadata.response_code = ResponseCode::FormErr;
    } else if request.metadata.op_code != OpCode::Query {
      request.metadata.response_code = ResponseCode::NotImp;
    } else if request.queries[0].query_class() != DNSClass::IN {
      request.metadata.response_code = ResponseCode::Refused;
    } else if request.queries[0].query_type() == RecordType::A {
      match self.allocate(&name) {
        Ok(ip) => {
          let reply = crate::dns::build_dns_response(request, &name, ip, 60)?;
          return Ok((reply.to_vec()?, name, Some(ip)));
        }
        Err(_) => request.metadata.response_code = ResponseCode::ServFail,
      }
    }
    // IPv4-only pool: AAAA/HTTPS/SVCB and unsupported types receive NODATA.
    Ok((request.to_vec()?, name, None))
  }
  fn allocate(&mut self, name: &str) -> anyhow::Result<IpAddr> {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if let Some(ip) = self.names.get(&name) {
      return Ok(*ip);
    }
    anyhow::ensure!(self.next < self.last, "virtual DNS address pool exhausted");
    let ip = if self.ipv4 {
      IpAddr::V4((self.next as u32).into())
    } else {
      IpAddr::V6(self.next.into())
    };
    self.next += 1;
    self.names.insert(name.clone(), ip);
    self.addresses.insert(ip, name);
    Ok(ip)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use hickory_proto::{
    op::{Message, Query},
    rr::Name,
  };
  fn query(name: &str, kind: RecordType) -> Vec<u8> {
    let mut msg = Message::query();
    msg.add_query(Query::query(Name::from_ascii(name).unwrap(), kind));
    msg.to_vec().unwrap()
  }
  #[test]
  fn only_a_queries_receive_ipv4_answers() {
    let mut dns = VirtualDns::new("198.18.0.0/15".parse().unwrap());
    for kind in [RecordType::AAAA, RecordType::HTTPS, RecordType::TXT] {
      let (data, _, ip) = dns.generate_query(&query("example.com.", kind)).unwrap();
      let reply = Message::from_vec(&data).unwrap();
      assert!(reply.answers.is_empty());
      assert!(ip.is_none());
      assert_eq!(reply.metadata.response_code, ResponseCode::NoError);
    }
  }
  #[test]
  fn mappings_stay_stable_and_normalize_case() {
    let mut dns = VirtualDns::new("198.18.0.0/15".parse().unwrap());
    let (_, _, a) = dns
      .generate_query(&query("EXAMPLE.com.", RecordType::A))
      .unwrap();
    let (_, _, b) = dns
      .generate_query(&query("example.com.", RecordType::A))
      .unwrap();
    assert_eq!(a, b);
    assert_eq!(dns.resolve_ip(&a.unwrap()).unwrap(), "example.com");
    assert_eq!(a.unwrap(), "198.18.0.1".parse::<IpAddr>().unwrap());
  }
  #[test]
  fn exhausted_pool_returns_servfail_without_reusing_addresses() {
    let mut dns = VirtualDns::new("198.18.0.0/30".parse().unwrap());
    let (_, _, first) = dns
      .generate_query(&query("one.test.", RecordType::A))
      .unwrap();
    dns
      .generate_query(&query("two.test.", RecordType::A))
      .unwrap();
    let (data, _, ip) = dns
      .generate_query(&query("three.test.", RecordType::A))
      .unwrap();
    assert!(ip.is_none());
    assert_eq!(
      Message::from_vec(&data).unwrap().metadata.response_code,
      ResponseCode::ServFail
    );
    assert_eq!(dns.resolve_ip(&first.unwrap()).unwrap(), "one.test");
  }
}
