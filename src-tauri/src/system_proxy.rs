use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[cfg(target_os = "macos")]
#[derive(Serialize, Deserialize)]
struct ProxySetting {
  enabled: bool,
  host: String,
  port: u16,
}
#[cfg(target_os = "macos")]
#[derive(Serialize, Deserialize)]
struct Service {
  name: String,
  http: ProxySetting,
  https: ProxySetting,
  bypass: Vec<String>,
}
#[cfg(target_os = "macos")]
#[derive(Serialize, Deserialize)]
struct Snapshot {
  #[serde(default = "legacy_port")]
  applied_port: u16,
  services: Vec<Service>,
}

#[cfg(target_os = "macos")]
fn network(args: &[&str]) -> Result<String> {
  let result = std::process::Command::new("/usr/sbin/networksetup")
    .args(args)
    .env("LC_ALL", "C")
    .output()?;
  let text = String::from_utf8_lossy(&result.stdout).into_owned();
  anyhow::ensure!(
    result.status.success() && !text.contains("** Error") && !text.contains("Error:"),
    "无法修改系统代理：{}",
    text.trim()
  );
  Ok(text)
}
#[cfg(target_os = "macos")]
fn read_proxy(service: &str, secure: bool) -> Result<ProxySetting> {
  let text = network(&[
    if secure {
      "-getsecurewebproxy"
    } else {
      "-getwebproxy"
    },
    service,
  ])?;
  let value = |key: &str| {
    text
      .lines()
      .find_map(|line| line.strip_prefix(key))
      .unwrap_or("")
      .trim()
      .to_owned()
  };
  anyhow::ensure!(
    value("Authenticated Proxy Enabled:") != "1",
    "现有系统代理需要鉴权，请手动设置代理以保留其凭据"
  );
  Ok(ProxySetting {
    enabled: value("Enabled:") == "Yes",
    host: value("Server:"),
    port: value("Port:").parse().unwrap_or(0),
  })
}
#[cfg(target_os = "macos")]
fn capture() -> Result<Snapshot> {
  let names = network(&["-listallnetworkservices"])?;
  let mut services = Vec::new();
  for name in names
    .lines()
    .skip(1)
    .filter(|line| !line.is_empty() && !line.starts_with('*'))
  {
    let bypass = network(&["-getproxybypassdomains", name])?
      .lines()
      .filter(|line| !line.is_empty() && !line.starts_with("There aren't"))
      .map(str::to_owned)
      .collect();
    services.push(Service {
      name: name.into(),
      http: read_proxy(name, false)?,
      https: read_proxy(name, true)?,
      bypass,
    });
  }
  anyhow::ensure!(!services.is_empty(), "没有可用的网络服务");
  Ok(Snapshot {
    applied_port: legacy_port(),
    services,
  })
}
#[cfg(target_os = "macos")]
fn set_proxy(name: &str, setting: &ProxySetting, secure: bool) -> Result<()> {
  let port = setting.port.to_string();
  if !setting.host.is_empty() && setting.port != 0 {
    network(&[
      if secure {
        "-setsecurewebproxy"
      } else {
        "-setwebproxy"
      },
      name,
      &setting.host,
      &port,
    ])?;
  }
  network(&[
    if secure {
      "-setsecurewebproxystate"
    } else {
      "-setwebproxystate"
    },
    name,
    if setting.enabled { "on" } else { "off" },
  ])?;
  Ok(())
}
#[cfg(target_os = "macos")]
fn apply(snapshot: &Snapshot, enable: bool) -> Result<()> {
  let own = ProxySetting {
    enabled: true,
    host: "127.0.0.1".into(),
    port: snapshot.applied_port,
  };
  let mut errors = Vec::new();
  for service in &snapshot.services {
    let result = (|| -> Result<()> {
      for secure in [false, true] {
        if enable {
          set_proxy(&service.name, &own, secure)?;
        } else {
          let current = read_proxy(&service.name, secure)?;
          // Do not overwrite proxy changes made by another application while connected.
          if current.host == own.host && current.port == own.port {
            set_proxy(
              &service.name,
              if secure {
                &service.https
              } else {
                &service.http
              },
              secure,
            )?;
          }
        }
      }
      let own_bypass = ["localhost", "127.0.0.1", "::1", "*.local"];
      if !enable {
        let current = network(&["-getproxybypassdomains", &service.name])?;
        if current.lines().collect::<Vec<_>>() != own_bypass {
          return Ok(());
        }
      }
      let domains = if enable {
        own_bypass.to_vec()
      } else {
        service.bypass.iter().map(String::as_str).collect()
      };
      let mut args = vec!["-setproxybypassdomains", &service.name];
      if domains.is_empty() {
        args.push("Empty");
      } else {
        args.extend(domains);
      }
      network(&args)?;
      Ok(())
    })();
    if let Err(e) = result {
      errors.push(e.to_string());
    }
  }
  anyhow::ensure!(errors.is_empty(), "{}", errors.join("；"));
  Ok(())
}

#[cfg(target_os = "windows")]
#[derive(Serialize, Deserialize)]
struct Snapshot {
  #[serde(default = "legacy_port")]
  applied_port: u16,
  enable: bool,
  host: String,
  port: u16,
  bypass: String,
}
#[cfg(target_os = "windows")]
fn capture() -> Result<Snapshot> {
  let p = sysproxy::Sysproxy::get_system_proxy()?;
  Ok(Snapshot {
    applied_port: legacy_port(),
    enable: p.enable,
    host: p.host,
    port: p.port,
    bypass: p.bypass,
  })
}
#[cfg(target_os = "windows")]
fn apply(snapshot: &Snapshot, enable: bool) -> Result<()> {
  if !enable {
    let current = sysproxy::Sysproxy::get_system_proxy()?;
    if current.host != "127.0.0.1" || current.port != snapshot.applied_port {
      return Ok(());
    }
  }
  let p = if enable {
    sysproxy::Sysproxy {
      enable: true,
      host: "127.0.0.1".into(),
      port: snapshot.applied_port,
      bypass: "localhost;127.*;[::1];<local>".into(),
    }
  } else {
    sysproxy::Sysproxy {
      enable: snapshot.enable,
      host: snapshot.host.clone(),
      port: snapshot.port,
      bypass: snapshot.bypass.clone(),
    }
  };
  p.set_system_proxy()?;
  Ok(())
}

pub fn enable(path: &Path, port: u16) -> Result<()> {
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  {
    if path.exists() {
      restore(path)?;
    }
    let mut snapshot = capture()?;
    snapshot.applied_port = port;
    crate::proxy::save_private(path, &serde_json::to_vec(&snapshot)?)?;
    if let Err(error) = apply(&snapshot, true) {
      let rollback = restore(path);
      return Err(error.context(format!(
          "启用系统代理失败；恢复结果：{}",
          rollback
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "已恢复".into())
        )));
    }
    return Ok(());
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  anyhow::bail!("当前平台暂不支持自动设置系统代理，请手动设置本机 HTTP 代理")
}
pub fn restore(path: &Path) -> Result<()> {
  if !path.exists() {
    return Ok(());
  }
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  {
    let snapshot: Snapshot =
      serde_json::from_slice(&fs::read(path)?).context("读取原系统代理配置失败")?;
    apply(&snapshot, false)?;
    fs::remove_file(path)?;
    return Ok(());
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  anyhow::bail!("当前平台无法恢复系统代理")
}

// Read backups written before configurable ports were introduced.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn legacy_port() -> u16 {
  7892
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
  use super::*;
  #[test]
  fn recovery_remembers_the_applied_port_and_reads_legacy_backups() {
    let mut snapshot: Snapshot = serde_json::from_str(r#"{"services":[]}"#).unwrap();
    assert_eq!(snapshot.applied_port, 7892);
    snapshot.applied_port = 17893;
    let restored: Snapshot =
      serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(restored.applied_port, 17893);
  }
}
