
pub fn hex2str(hex: &[u8]) -> String {
  hex
    .iter()
    .map(|n| format!("{:02x}", n))
    .collect::<Vec<String>>()
    .join(",")
}
