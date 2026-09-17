pub fn join(base: &str, name: &str) -> String {
  if name.starts_with('/') {
    return name.to_string();
  }
  if base.is_empty() || base == "/" {
    format!("/{name}")
  } else if base.ends_with('/') {
    format!("{base}{name}")
  } else {
    format!("{base}/{name}")
  }
}

pub fn parent(path: &str) -> String {
  let trimmed = path.trim_end_matches('/');
  match trimmed.rfind('/') {
    Some(0) | None => "/".to_string(),
    Some(i) => trimmed[..i].to_string(),
  }
}

pub fn basename(path: &str) -> String {
  let trimmed = path.trim_end_matches('/');
  match trimmed.rfind('/') {
    Some(i) => trimmed[i + 1..].to_string(),
    None => trimmed.to_string(),
  }
}

pub fn normalize(path: &str) -> String {
  let mut parts: Vec<&str> = Vec::new();
  for segment in path.split('/') {
    match segment {
      "" | "." => {}
      ".." => {
        parts.pop();
      }
      s => parts.push(s),
    }
  }
  if parts.is_empty() {
    "/".to_string()
  } else {
    format!("/{}", parts.join("/"))
  }
}

pub fn resolve_link_target(link_path: &str, target: &str) -> String {
  if target.starts_with('/') {
    normalize(target)
  } else {
    normalize(&join(&parent(link_path), target))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn joins_and_normalizes() {
    assert_eq!(join("/", "a"), "/a");
    assert_eq!(join("/x", "a"), "/x/a");
    assert_eq!(join("/x/", "a"), "/x/a");
    assert_eq!(normalize("/a/./b/../c"), "/a/c");
    assert_eq!(parent("/a/b"), "/a");
    assert_eq!(parent("/a"), "/");
    assert_eq!(basename("/a/b.txt"), "b.txt");
    assert_eq!(resolve_link_target("/usr/bin/x", "../lib/y"), "/usr/lib/y");
  }
}
