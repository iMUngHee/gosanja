use std::collections::BTreeSet;
use std::path::PathBuf;

fn paths(text: &str) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    let mut path = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ':' => return None,
            ch if ch.is_control() => return None,
            '\\' if chars.peek() == Some(&' ') => {
                chars.next();
                path.push(' ');
            }
            ' ' if !path.is_empty() => paths.push(std::mem::take(&mut path)),
            ' ' => (),
            ch => path.push(ch),
        }
    }
    if !path.is_empty() {
        paths.push(path);
    }
    Some(paths)
}

/// Parse rustc's dep-info format. Check the ordered phony rules before deduplication:
/// a newline in a path is written verbatim by rustc and can otherwise look like two paths.
#[must_use]
pub fn dep_info(text: &str, outputs: &[PathBuf]) -> Option<Vec<PathBuf>> {
    let mut lines = text.split('\n').filter(|line| !line.is_empty());
    let mut remaining: BTreeSet<_> = outputs.iter().cloned().collect();
    let mut prerequisites = None;
    for _ in outputs {
        let (output, listed) = lines.next()?.split_once(':')?;
        let [output] = paths(output)?.try_into().ok()?;
        if !remaining.remove(&PathBuf::from(output)) {
            return None;
        }
        let listed = paths(listed.strip_prefix(' ')?)?;
        if listed.iter().collect::<BTreeSet<_>>().len() != listed.len()
            || prerequisites.as_ref().is_some_and(|prior| *prior != listed)
        {
            return None;
        }
        prerequisites = Some(listed);
    }
    let prerequisites = prerequisites?;
    for expected in &prerequisites {
        let [actual] = paths(lines.next()?.strip_suffix(':')?)?.try_into().ok()?;
        if &actual != expected {
            return None;
        }
    }
    if !lines.all(|line| line.starts_with("# env-dep:") || line.starts_with("# checksum:")) {
        return None;
    }
    Some(prerequisites.into_iter().map(PathBuf::from).collect())
}
