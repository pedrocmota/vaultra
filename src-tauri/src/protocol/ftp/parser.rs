use crate::protocol::{join, EntryKind, RemoteEntry};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};

const MONTHS: [&str; 12] = [
  "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

pub fn parse_mlsd_line(dir: &str, line: &str) -> Option<RemoteEntry> {
  let (facts, name) = line.split_once(' ')?;
  let name = name.trim_end_matches(['\r', '\n']);
  if name.is_empty() || name == "." || name == ".." {
    return None;
  }
  let mut entry = RemoteEntry::plain(&join(dir, name), EntryKind::File, 0, None);
  entry.name = name.to_string();
  for fact in facts.split(';') {
    let Some((key, value)) = fact.split_once('=') else {
      continue;
    };
    match key.to_ascii_lowercase().as_str() {
      "type" => {
        let lower = value.to_ascii_lowercase();
        entry.kind = match lower.as_str() {
          "dir" => EntryKind::Dir,
          "cdir" | "pdir" => return None,
          "file" => EntryKind::File,
          _ if lower.starts_with("os.unix=symlink") || lower.starts_with("os.unix=slink") => {
            entry.link_target = value.split_once(':').map(|(_, target)| target.to_string());
            EntryKind::Symlink
          }
          _ => EntryKind::Other,
        };
      }
      "size" | "sizd" => entry.size = value.parse().unwrap_or(0),
      "modify" => entry.mtime = parse_mlsd_time(value),
      "unix.mode" => entry.permissions = u32::from_str_radix(value, 8).ok().map(|m| m & 0o7777),
      "unix.owner" => entry.owner = Some(value.to_string()),
      "unix.uid" if entry.owner.is_none() => entry.owner = Some(value.to_string()),
      "unix.group" => entry.group = Some(value.to_string()),
      "unix.gid" if entry.group.is_none() => entry.group = Some(value.to_string()),
      _ => {}
    }
  }
  Some(entry)
}

pub fn parse_mlsd_time(value: &str) -> Option<i64> {
  let digits = value.split('.').next()?;
  if digits.len() < 14 {
    return None;
  }
  let field = |range: std::ops::Range<usize>| digits[range].parse::<u32>().ok();
  let year: i32 = digits[0..4].parse().ok()?;
  let date = NaiveDate::from_ymd_opt(year, field(4..6)?, field(6..8)?)?;
  let datetime = date.and_hms_opt(field(8..10)?, field(10..12)?, field(12..14)?)?;
  Some(Utc.from_utc_datetime(&datetime).timestamp())
}

pub fn parse_list_line(dir: &str, line: &str) -> Option<RemoteEntry> {
  let line = line.trim_end_matches(['\r', '\n']);
  if line.is_empty() || line.starts_with("total ") {
    return None;
  }
  parse_unix_line(dir, line).or_else(|| parse_dos_line(dir, line))
}

fn parse_unix_line(dir: &str, line: &str) -> Option<RemoteEntry> {
  let type_char = line.chars().next()?;
  if !"-dlcbps".contains(type_char) {
    return None;
  }
  let tokens: Vec<&str> = line.split_whitespace().collect();
  if tokens.len() < 7 {
    return None;
  }
  let month_index = (3..tokens.len().saturating_sub(2)).find(|&i| {
    MONTHS.contains(&tokens[i].to_ascii_lowercase().as_str())
      && tokens[i + 1].parse::<u32>().is_ok()
      && (tokens[i + 2].contains(':') || tokens[i + 2].len() == 4)
  })?;
  let size: u64 = tokens[month_index - 1].parse().unwrap_or(0);
  let (owner, group) = match month_index {
    i if i >= 5 => (
      Some(tokens[i - 3].to_string()),
      Some(tokens[i - 2].to_string()),
    ),
    4 => (Some(tokens[2].to_string()), None),
    _ => (None, None),
  };
  let mtime = parse_unix_date(
    tokens[month_index],
    tokens[month_index + 1],
    tokens[month_index + 2],
  );
  let name_part = remainder_after_token(line, month_index + 2)?;
  let (name, link_target) = match (type_char, name_part.split_once(" -> ")) {
    ('l', Some((name, target))) => (name.to_string(), Some(target.to_string())),
    _ => (name_part.to_string(), None),
  };
  if name.is_empty() || name == "." || name == ".." {
    return None;
  }
  let kind = match type_char {
    'd' => EntryKind::Dir,
    'l' => EntryKind::Symlink,
    '-' => EntryKind::File,
    _ => EntryKind::Other,
  };
  Some(RemoteEntry {
    path: join(dir, &name),
    name,
    kind,
    size,
    mtime,
    permissions: parse_permission_string(tokens[0]),
    owner,
    group,
    link_target,
    link_broken: false,
    target_is_dir: None,
  })
}

fn remainder_after_token(line: &str, index: usize) -> Option<&str> {
  let mut seen = 0;
  let mut in_token = false;
  for (i, c) in line.char_indices() {
    match (c.is_whitespace(), in_token) {
      (true, true) => {
        if seen == index {
          let rest = &line[i..];
          return Some(rest.strip_prefix(' ').unwrap_or(rest));
        }
        seen += 1;
        in_token = false;
      }
      (false, false) => in_token = true,
      _ => {}
    }
  }
  None
}

fn parse_permission_string(text: &str) -> Option<u32> {
  let bytes = text.as_bytes();
  if bytes.len() < 10 {
    return None;
  }
  let mut mode = 0u32;
  let bits = [
    0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
  ];
  for (offset, bit) in bits.iter().enumerate() {
    let c = bytes[offset + 1] as char;
    if !matches!(c, '-' | 'S' | 'T') {
      mode |= bit;
    }
  }
  if matches!(bytes[3] as char, 's' | 'S') {
    mode |= 0o4000;
  }
  if matches!(bytes[6] as char, 's' | 'S') {
    mode |= 0o2000;
  }
  if matches!(bytes[9] as char, 't' | 'T') {
    mode |= 0o1000;
  }
  Some(mode)
}

fn parse_unix_date(month: &str, day: &str, time_or_year: &str) -> Option<i64> {
  let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(month))? as u32 + 1;
  let day: u32 = day.parse().ok()?;
  let now = Utc::now();
  let (year, hour, minute) = match time_or_year.split_once(':') {
    Some((h, m)) => {
      let mut year = now.year();
      if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
        if date > now.date_naive() + chrono::Duration::days(1) {
          year -= 1;
        }
      }
      (year, h.parse().ok()?, m.parse().ok()?)
    }
    None => (time_or_year.parse().ok()?, 0, 0),
  };
  let datetime = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
  Some(Utc.from_utc_datetime(&datetime).timestamp())
}

fn parse_dos_line(dir: &str, line: &str) -> Option<RemoteEntry> {
  let tokens: Vec<&str> = line.split_whitespace().collect();
  if tokens.len() < 4 {
    return None;
  }
  let date_parts: Vec<&str> = tokens[0].split('-').collect();
  if date_parts.len() != 3 {
    return None;
  }
  let month: u32 = date_parts[0].parse().ok()?;
  let day: u32 = date_parts[1].parse().ok()?;
  let mut year: i32 = date_parts[2].parse().ok()?;
  if year < 100 {
    year += if year < 70 { 2000 } else { 1900 };
  }
  let time = tokens[1];
  let (clock, meridiem) = match time.strip_suffix("PM") {
    Some(t) => (t, Some(true)),
    None => match time.strip_suffix("AM") {
      Some(t) => (t, Some(false)),
      None => (time, None),
    },
  };
  let (hour, minute) = clock.split_once(':')?;
  let mut hour: u32 = hour.parse().ok()?;
  let minute: u32 = minute.parse().ok()?;
  match meridiem {
    Some(true) if hour < 12 => hour += 12,
    Some(false) if hour == 12 => hour = 0,
    _ => {}
  }
  let mtime = NaiveDate::from_ymd_opt(year, month, day)?
    .and_hms_opt(hour, minute, 0)
    .map(|dt| Utc.from_utc_datetime(&dt).timestamp());
  let (kind, size) = if tokens[2].eq_ignore_ascii_case("<DIR>") {
    (EntryKind::Dir, 0)
  } else {
    (EntryKind::File, tokens[2].parse().ok()?)
  };
  let name = remainder_after_token(line, 2)?.trim().to_string();
  if name.is_empty() || name == "." || name == ".." {
    return None;
  }
  let mut entry = RemoteEntry::plain(&join(dir, &name), kind, size, mtime);
  entry.name = name;
  Some(entry)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_unix_file() {
    let e = parse_list_line(
      "/home",
      "-rw-r--r--   1 pedro users     1234 Jan 15 12:34 my file.txt",
    )
    .unwrap();
    assert_eq!(e.name, "my file.txt");
    assert_eq!(e.size, 1234);
    assert_eq!(e.permissions, Some(0o644));
    assert_eq!(e.owner.as_deref(), Some("pedro"));
    assert_eq!(e.path, "/home/my file.txt");
  }

  #[test]
  fn parses_unix_symlink() {
    let e = parse_list_line("/", "lrwxrwxrwx 1 root root 7 Mar  3  2021 bin -> usr/bin").unwrap();
    assert_eq!(e.kind, EntryKind::Symlink);
    assert_eq!(e.name, "bin");
    assert_eq!(e.link_target.as_deref(), Some("usr/bin"));
  }

  #[test]
  fn parses_dos_lines() {
    let d = parse_list_line("/", "01-15-23  12:34PM       <DIR>          Program Files").unwrap();
    assert_eq!(d.kind, EntryKind::Dir);
    assert_eq!(d.name, "Program Files");
    let f = parse_list_line("/", "01-15-23  01:02AM                 42 a.txt").unwrap();
    assert_eq!(f.size, 42);
  }

  #[test]
  fn parses_mlsd_lines() {
    let e = parse_mlsd_line(
      "/x",
      "type=file;size=10;modify=20230115123456;UNIX.mode=0644;UNIX.owner=u;UNIX.group=g; a b.txt",
    )
    .unwrap();
    assert_eq!(e.name, "a b.txt");
    assert_eq!(e.permissions, Some(0o644));
    assert!(e.mtime.is_some());
    let l = parse_mlsd_line("/x", "type=OS.unix=slink:/tmp;size=3; lnk").unwrap();
    assert_eq!(l.kind, EntryKind::Symlink);
    assert_eq!(l.link_target.as_deref(), Some("/tmp"));
  }
}
