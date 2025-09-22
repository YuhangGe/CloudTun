use anyhow::{anyhow, bail};
use hickory_proto::{
  op::{Message, MessageType},
  rr::{
    Name, RData, Record,
    rdata::{A, AAAA},
  },
};
use std::{net::IpAddr, str::FromStr};

pub fn build_dns_response(
  mut request: Message,
  domain: &str,
  ip: IpAddr,
  ttl: u32,
) -> anyhow::Result<Message> {
  let record = match ip {
    IpAddr::V4(ip) => Record::from_rdata(Name::from_str(domain)?, ttl, RData::A(A(ip))),
    IpAddr::V6(ip) => Record::from_rdata(Name::from_str(domain)?, ttl, RData::AAAA(AAAA(ip))),
  };

  // We must indicate that this message is a response. Otherwise, implementations may not
  // recognize it.
  request.set_message_type(MessageType::Response);

  request.add_answer(record);
  Ok(request)
}

// pub fn remove_ipv6_entries(message: &mut Message) {
//   message
//     .answers_mut()
//     .retain(|answer| !matches!(answer.data(), RData::AAAA(_)));
// }

// pub fn extract_ipaddr_from_dns_message(message: &Message) -> anyhow::Result<IpAddr> {
//   if message.response_code() != ResponseCode::NoError {
//     bail!("{:?}", message.response_code());
//   }
//   let mut cname = None;
//   for answer in message.answers() {
//     match answer.data() {
//       RData::A(addr) => {
//         return Ok(IpAddr::V4((*addr).into()));
//       }
//       RData::AAAA(addr) => {
//         return Ok(IpAddr::V6((*addr).into()));
//       }
//       RData::CNAME(name) => {
//         cname = Some(name.to_utf8());
//       }
//       _ => {}
//     }
//   }
//   if let Some(cname) = cname {
//     bail!(cname)
//   }
//   bail!("{:?}", message.answers())
// }

pub fn extract_domain_from_dns_message(message: &Message) -> anyhow::Result<String> {
  let query = message
    .queries()
    .first()
    .ok_or(anyhow!("DnsRequest no query body"))?;
  let name = query.name().to_string();
  Ok(name)
}

pub fn parse_data_to_dns_message(data: &[u8], used_by_tcp: bool) -> anyhow::Result<Message> {
  if used_by_tcp {
    if data.len() < 2 {
      bail!("invalid dns data");
    }
    let len = u16::from_be_bytes([data[0], data[1]]) as usize;
    let data = data.get(2..len + 2).ok_or(anyhow!("invalid dns data"))?;
    return parse_data_to_dns_message(data, false);
  }
  let message = Message::from_vec(data).map_err(|e| anyhow!(e.to_string()))?;
  Ok(message)
}
