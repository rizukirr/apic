//! The naming rules for new requests and templates, as pure functions.

/// What a new-request input names, relative to the contracts root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestTarget {
    /// The relative path, without a trailing `/` for a folder.
    pub(crate) rel: String,

    /// Whether the input names a folder (it ended in `/`).
    pub(crate) is_folder: bool,
}

/// Parses the new-request dialog input. A name ending in `/` is a folder, a
/// name ending in `.json` is kept, any other name gets `.json` appended.
/// Blank input, or input that is only `/`, is refused with "name required".
pub(crate) fn request_target(input: &str) -> Result<RequestTarget, String> {
    let input = input.trim();
    let is_folder = input.ends_with('/');
    let rel = if is_folder {
        input.trim_end_matches('/').to_string()
    } else if input.ends_with(".json") {
        input.to_string()
    } else {
        format!("{input}.json")
    };
    if input.is_empty() || rel.is_empty() {
        return Err("name required".into());
    }
    Ok(RequestTarget { rel, is_folder })
}

/// The file name for a new template: the trimmed name with `.json` appended
/// unless it already ends in it. Blank is refused with "template name required".
pub(crate) fn template_file_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("template name required".into());
    }
    Ok(if name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{name}.json")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_target_follows_the_path_rule() {
        assert_eq!(
            request_target("a"),
            Ok(RequestTarget {
                rel: "a.json".into(),
                is_folder: false
            })
        );
        assert_eq!(
            request_target("a/"),
            Ok(RequestTarget {
                rel: "a".into(),
                is_folder: true
            })
        );
        assert_eq!(
            request_target("a.json"),
            Ok(RequestTarget {
                rel: "a.json".into(),
                is_folder: false
            })
        );
        assert!(request_target(" ").is_err());
        assert!(request_target("/").is_err());
    }

    #[test]
    fn template_file_name_appends_json_once() {
        assert_eq!(template_file_name("x"), Ok("x.json".to_string()));
        assert_eq!(template_file_name("x.json"), Ok("x.json".to_string()));
        assert!(template_file_name(" ").is_err());
    }
}
