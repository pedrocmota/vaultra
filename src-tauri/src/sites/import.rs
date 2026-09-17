use super::model::{LogonType, SiteConfig, SiteNode};
use crate::error::{VError, VResult};
use crate::protocol::ftp::client::{EncodingMode, TransferMode};
use crate::protocol::Protocol;
use base64::Engine;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Clone)]
struct XmlNode {
  name: String,
  text: String,
  children: Vec<XmlNode>,
}

impl XmlNode {
  fn child(&self, name: &str) -> Option<&XmlNode> {
    self.children.iter().find(|c| c.name == name)
  }

  fn child_text(&self, name: &str) -> String {
    self
      .child(name)
      .map(|c| c.text.trim().to_string())
      .unwrap_or_default()
  }
}

fn parse_xml(text: &str) -> VResult<XmlNode> {
  let mut reader = Reader::from_str(text);
  reader.config_mut().trim_text(false);
  let mut stack: Vec<XmlNode> = vec![XmlNode {
    name: "#root".into(),
    ..Default::default()
  }];
  loop {
    match reader.read_event() {
      Ok(Event::Start(e)) => {
        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
        stack.push(XmlNode {
          name,
          ..Default::default()
        });
      }
      Ok(Event::Empty(e)) => {
        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
        stack.last_mut().unwrap().children.push(XmlNode {
          name,
          ..Default::default()
        });
      }
      Ok(Event::Text(t)) => {
        if let Ok(s) = t.unescape() {
          stack.last_mut().unwrap().text.push_str(&s);
        }
      }
      Ok(Event::CData(t)) => {
        stack
          .last_mut()
          .unwrap()
          .text
          .push_str(&String::from_utf8_lossy(&t));
      }
      Ok(Event::End(_)) => {
        let node = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(node);
      }
      Ok(Event::Eof) => break,
      Err(e) => return Err(VError::Other(format!("invalid XML: {e}"))),
      _ => {}
    }
  }
  Ok(stack.pop().unwrap())
}

pub struct ImportedSite {
  pub site: SiteConfig,
  pub password: Option<String>,
}

pub struct ImportResult {
  pub nodes: Vec<SiteNode>,
  pub passwords: Vec<(String, String, String)>,
}

pub fn import_filezilla(path: &Path) -> VResult<ImportResult> {
  let text = std::fs::read_to_string(path)
    .map_err(|e| VError::Io(format!("cannot read {}: {e}", path.display())))?;
  let root = parse_xml(&text)?;
  let servers = root
    .child("FileZilla3")
    .and_then(|f| f.child("Servers"))
    .ok_or_else(|| VError::Other("not a FileZilla site manager file".into()))?;
  let mut passwords = Vec::new();
  let nodes = convert_filezilla_children(servers, &mut passwords);
  Ok(ImportResult { nodes, passwords })
}

fn convert_filezilla_children(
  node: &XmlNode,
  passwords: &mut Vec<(String, String, String)>,
) -> Vec<SiteNode> {
  let mut out = Vec::new();
  for child in &node.children {
    match child.name.as_str() {
      "Folder" => {
        let name = child.text.trim().to_string();
        let children = convert_filezilla_children(child, passwords);
        out.push(SiteNode::folder(
          if name.is_empty() {
            "Folder".into()
          } else {
            name
          },
          children,
        ));
      }
      "Server" => {
        let imported = convert_filezilla_server(child);
        if let Some(pw) = imported.password {
          passwords.push((imported.site.id.clone(), imported.site.user.clone(), pw));
        }
        out.push(SiteNode::site(imported.site));
      }
      _ => {}
    }
  }
  out
}

fn convert_filezilla_server(node: &XmlNode) -> ImportedSite {
  let mut site = SiteConfig::default();
  site.host = node.child_text("Host");
  site.port = node.child_text("Port").parse().unwrap_or(0);
  site.protocol = match node.child_text("Protocol").as_str() {
    "1" => Protocol::Sftp,
    "3" => Protocol::FtpsImplicit,
    "4" => Protocol::FtpsExplicit,
    _ => Protocol::Ftp,
  };
  if site.port == 0 {
    site.port = site.protocol.default_port();
  }
  site.user = node.child_text("User");
  site.logon_type = match node.child_text("Logontype").as_str() {
    "0" => LogonType::Anonymous,
    "2" => LogonType::Ask,
    "3" => LogonType::Interactive,
    "5" => LogonType::KeyFile,
    _ => LogonType::Normal,
  };
  site.transfer_mode = match node.child_text("PasvMode").as_str() {
    "MODE_PASSIVE" => TransferMode::Passive,
    "MODE_ACTIVE" => TransferMode::Active,
    _ => TransferMode::Default,
  };
  site.encoding = match node.child_text("EncodingType").as_str() {
    "UTF-8" => EncodingMode::Utf8,
    "Custom" => EncodingMode::Custom(node.child_text("CustomEncoding")),
    _ => EncodingMode::Auto,
  };
  site.bypass_proxy = node.child_text("BypassProxy") == "1";
  let name = node.child_text("Name");
  site.name = if name.is_empty() {
    site.host.clone()
  } else {
    name
  };
  site.comments = node.child_text("Comments");
  site.local_dir = node.child_text("LocalDir");
  site.remote_dir = decode_filezilla_remote_dir(&node.child_text("RemoteDir"));
  site.sync_browsing = node.child_text("SyncBrowsing") == "1";
  site.color = match node.child_text("Colour").as_str() {
    "1" => Some("#e5484d".into()),
    "2" => Some("#30a46c".into()),
    "3" => Some("#3e63dd".into()),
    "4" => Some("#f5d90a".into()),
    "5" => Some("#12a594".into()),
    "6" => Some("#d6409f".into()),
    "7" => Some("#f76b15".into()),
    _ => None,
  };
  site.sftp.key_path = node.child_text("Keyfile");
  let password = node.child("Pass").and_then(|p| {
    let raw = p.text.trim();
    if raw.is_empty() {
      return None;
    }
    base64::engine::general_purpose::STANDARD
      .decode(raw)
      .ok()
      .map(|b| String::from_utf8_lossy(&b).into_owned())
      .or_else(|| Some(raw.to_string()))
  });
  ImportedSite { site, password }
}

fn decode_filezilla_remote_dir(raw: &str) -> String {
  let mut chars = raw.chars().peekable();
  let mut tokens_skipped = 0;
  let mut segments: Vec<String> = Vec::new();
  while tokens_skipped < 2 {
    let mut token = String::new();
    while let Some(&c) = chars.peek() {
      if c == ' ' {
        chars.next();
        break;
      }
      token.push(c);
      chars.next();
    }
    if token.is_empty() && chars.peek().is_none() {
      return String::new();
    }
    tokens_skipped += 1;
  }
  loop {
    let mut len_text = String::new();
    while let Some(&c) = chars.peek() {
      if c == ' ' {
        chars.next();
        break;
      }
      len_text.push(c);
      chars.next();
    }
    let Ok(len) = len_text.parse::<usize>() else {
      break;
    };
    let segment: String = chars.by_ref().take(len).collect();
    segments.push(segment);
    if chars.peek() == Some(&' ') {
      chars.next();
    }
    if chars.peek().is_none() {
      break;
    }
  }
  if segments.is_empty() {
    String::new()
  } else {
    format!("/{}", segments.join("/"))
  }
}

pub fn import_winscp() -> VResult<ImportResult> {
  let mut sessions: Vec<(String, HashMap<String, String>)> = Vec::new();
  #[cfg(windows)]
  sessions.extend(read_winscp_registry());
  sessions.extend(read_winscp_ini());
  if sessions.is_empty() {
    return Err(VError::NotFound("no WinSCP sessions found".into()));
  }
  let mut folders: HashMap<String, Vec<SiteNode>> = HashMap::new();
  let mut root: Vec<SiteNode> = Vec::new();
  for (full_name, values) in sessions {
    let decoded = percent_decode(&full_name);
    let (folder, name) = match decoded.rsplit_once('/') {
      Some((f, n)) => (Some(f.to_string()), n.to_string()),
      None => (None, decoded.clone()),
    };
    let site = convert_winscp_session(&name, &values);
    match folder {
      Some(f) => folders.entry(f).or_default().push(SiteNode::site(site)),
      None => root.push(SiteNode::site(site)),
    }
  }
  let mut folder_names: Vec<String> = folders.keys().cloned().collect();
  folder_names.sort();
  for name in folder_names {
    let children = folders.remove(&name).unwrap_or_default();
    root.push(SiteNode::folder(name, children));
  }
  Ok(ImportResult {
    nodes: root,
    passwords: Vec::new(),
  })
}

fn convert_winscp_session(name: &str, values: &HashMap<String, String>) -> SiteConfig {
  let get = |key: &str| {
    values
      .get(&key.to_ascii_lowercase())
      .cloned()
      .unwrap_or_default()
  };
  let mut site = SiteConfig::default();
  site.name = name.to_string();
  site.host = get("HostName");
  site.user = get("UserName");
  let fs_protocol = get("FSProtocol").parse::<u32>().unwrap_or(0);
  let ftps = get("Ftps").parse::<u32>().unwrap_or(0);
  site.protocol = match fs_protocol {
    5 => match ftps {
      1 => Protocol::FtpsImplicit,
      2 | 3 => Protocol::FtpsExplicit,
      _ => Protocol::Ftp,
    },
    _ => Protocol::Sftp,
  };
  site.port = get("PortNumber").parse().unwrap_or(0);
  if site.port == 0 {
    site.port = site.protocol.default_port();
  }
  site.remote_dir = get("RemoteDirectory");
  site.local_dir = get("LocalDirectory");
  site.sftp.key_path = get("PublicKeyFile");
  site.transfer_mode = match get("FtpPasvMode").as_str() {
    "0" => TransferMode::Active,
    _ => TransferMode::Default,
  };
  site.logon_type = if !site.sftp.key_path.is_empty() {
    LogonType::KeyFile
  } else if site.user.is_empty() && site.protocol.is_ftp_family() {
    LogonType::Anonymous
  } else {
    LogonType::Ask
  };
  site
}

fn percent_decode(text: &str) -> String {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' && i + 2 < bytes.len() {
      if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
        out.push(v);
        i += 3;
        continue;
      }
    }
    out.push(bytes[i]);
    i += 1;
  }
  String::from_utf8_lossy(&out).into_owned()
}

#[cfg(windows)]
fn read_winscp_registry() -> Vec<(String, HashMap<String, String>)> {
  use winreg::enums::HKEY_CURRENT_USER;
  use winreg::RegKey;
  let mut out = Vec::new();
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let Ok(sessions) = hkcu.open_subkey(r"Software\Martin Prikryl\WinSCP 2\Sessions") else {
    return out;
  };
  for name in sessions.enum_keys().filter_map(Result::ok) {
    if name == "Default%20Settings" {
      continue;
    }
    let Ok(key) = sessions.open_subkey(&name) else {
      continue;
    };
    let mut values = HashMap::new();
    for (value_name, value) in key.enum_values().filter_map(Result::ok) {
      let text = match value.vtype {
        winreg::enums::RegType::REG_DWORD => {
          u32::from_le_bytes(value.bytes[..4].try_into().unwrap_or([0; 4])).to_string()
        }
        _ => value.to_string(),
      };
      values.insert(value_name.to_ascii_lowercase(), text);
    }
    if values.contains_key("hostname") {
      out.push((name, values));
    }
  }
  out
}

fn read_winscp_ini() -> Vec<(String, HashMap<String, String>)> {
  let mut candidates = Vec::new();
  if let Some(appdata) = dirs::config_dir() {
    candidates.push(appdata.join("WinSCP.ini"));
  }
  if let Ok(pf) = std::env::var("ProgramFiles(x86)") {
    candidates.push(Path::new(&pf).join("WinSCP").join("WinSCP.ini"));
  }
  if let Ok(pf) = std::env::var("ProgramFiles") {
    candidates.push(Path::new(&pf).join("WinSCP").join("WinSCP.ini"));
  }
  let mut out = Vec::new();
  for path in candidates {
    let Ok(text) = std::fs::read_to_string(&path) else {
      continue;
    };
    let mut current: Option<(String, HashMap<String, String>)> = None;
    for line in text.lines() {
      let line = line.trim();
      if line.starts_with('[') && line.ends_with(']') {
        if let Some(section) = current.take() {
          if section.1.contains_key("hostname") {
            out.push(section);
          }
        }
        let section = &line[1..line.len() - 1];
        current = section
          .strip_prefix("Sessions\\")
          .map(|name| (name.to_string(), HashMap::new()));
      } else if let (Some((_, values)), Some((k, v))) = (current.as_mut(), line.split_once('=')) {
        values.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
      }
    }
    if let Some(section) = current.take() {
      if section.1.contains_key("hostname") {
        out.push(section);
      }
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decodes_filezilla_remote_dir() {
    assert_eq!(
      decode_filezilla_remote_dir("1 0 4 home 5 pedro"),
      "/home/pedro"
    );
    assert_eq!(decode_filezilla_remote_dir("1 0 7 my docs"), "/my docs");
    assert_eq!(decode_filezilla_remote_dir(""), "");
  }

  #[test]
  fn parses_filezilla_xml() {
    let xml = r#"<?xml version="1.0"?><FileZilla3><Servers><Folder expanded="1">Work<Server><Host>h</Host><Port>22</Port><Protocol>1</Protocol><User>u</User><Logontype>5</Logontype><Name>S</Name><Keyfile>C:\k</Keyfile></Server></Folder></Servers></FileZilla3>"#;
    let root = parse_xml(xml).unwrap();
    let servers = root.child("FileZilla3").unwrap().child("Servers").unwrap();
    let mut pw = Vec::new();
    let nodes = convert_filezilla_children(servers, &mut pw);
    match &nodes[0] {
      SiteNode::Folder { name, children, .. } => {
        assert_eq!(name, "Work");
        match &children[0] {
          SiteNode::Site { site, .. } => {
            assert_eq!(site.protocol, Protocol::Sftp);
            assert_eq!(site.logon_type, LogonType::KeyFile);
            assert_eq!(site.sftp.key_path, r"C:\k");
          }
          _ => panic!(),
        }
      }
      _ => panic!(),
    }
  }
}
