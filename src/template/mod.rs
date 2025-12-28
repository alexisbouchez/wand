use std::collections::HashMap;

pub fn render(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    // Handle {{ var }} substitution
    while let Some(start) = result.find("{{") {
        if let Some(end) = result[start..].find("}}") {
            let end = start + end + 2;
            let expr = &result[start + 2..end - 2].trim();

            let value = if let Some((var_name, filter_chain)) = expr.split_once('|') {
                let var_name = var_name.trim();
                let base_value = vars.get(var_name).cloned().unwrap_or_default();
                apply_filters(&base_value, filter_chain, vars)
            } else {
                vars.get(*expr).cloned().unwrap_or_default()
            };

            result = format!("{}{}{}", &result[..start], value, &result[end..]);
        } else {
            break;
        }
    }

    result
}

fn apply_filters(value: &str, filter_chain: &str, vars: &HashMap<String, String>) -> String {
    let mut result = value.to_string();

    for filter in filter_chain.split('|') {
        let filter = filter.trim();

        if let Some((name, args)) = filter.split_once('(') {
            let args = args.trim_end_matches(')');
            result = apply_filter_with_args(&result, name.trim(), args, vars);
        } else {
            result = apply_filter(&result, filter);
        }
    }

    result
}

fn apply_filter(value: &str, filter: &str) -> String {
    match filter {
        "lower" => value.to_lowercase(),
        "upper" => value.to_uppercase(),
        "capitalize" => {
            let mut chars = value.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        }
        "trim" => value.trim().to_string(),
        "length" => value.len().to_string(),
        _ => value.to_string(),
    }
}

fn apply_filter_with_args(value: &str, filter: &str, args: &str, vars: &HashMap<String, String>) -> String {
    let args = args.trim();
    let arg = if args.starts_with('"') || args.starts_with('\'') {
        args.trim_matches(|c| c == '"' || c == '\'').to_string()
    } else {
        vars.get(args).cloned().unwrap_or(args.to_string())
    };

    match filter {
        "default" => {
            if value.is_empty() {
                arg
            } else {
                value.to_string()
            }
        }
        "replace" => {
            if let Some((old, new)) = arg.split_once(',') {
                let old = old.trim().trim_matches(|c| c == '"' || c == '\'');
                let new = new.trim().trim_matches(|c| c == '"' || c == '\'');
                value.replace(old, new)
            } else {
                value.to_string()
            }
        }
        "join" => value.to_string(), // For lists, handled separately
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn simple_substitution() {
        let result = render("Hello {{ name }}!", &vars(&[("name", "World")]));
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn multiple_substitutions() {
        let result = render("{{ greeting }} {{ name }}!", &vars(&[("greeting", "Hello"), ("name", "World")]));
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn missing_var_empty() {
        let result = render("Hello {{ name }}!", &vars(&[]));
        assert_eq!(result, "Hello !");
    }

    #[test]
    fn filter_lower() {
        let result = render("{{ name | lower }}", &vars(&[("name", "HELLO")]));
        assert_eq!(result, "hello");
    }

    #[test]
    fn filter_upper() {
        let result = render("{{ name | upper }}", &vars(&[("name", "hello")]));
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn filter_capitalize() {
        let result = render("{{ name | capitalize }}", &vars(&[("name", "hello")]));
        assert_eq!(result, "Hello");
    }

    #[test]
    fn filter_default() {
        let result = render("{{ missing | default('fallback') }}", &vars(&[]));
        assert_eq!(result, "fallback");
    }

    #[test]
    fn filter_default_not_used() {
        let result = render("{{ name | default('fallback') }}", &vars(&[("name", "value")]));
        assert_eq!(result, "value");
    }

    #[test]
    fn filter_chain() {
        let result = render("{{ name | upper | default('NONE') }}", &vars(&[("name", "hello")]));
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn filter_trim() {
        let result = render("{{ name | trim }}", &vars(&[("name", "  hello  ")]));
        assert_eq!(result, "hello");
    }

    #[test]
    fn filter_replace() {
        let result = render("{{ text | replace('old', 'new') }}", &vars(&[("text", "old value")]));
        assert_eq!(result, "new value");
    }

    #[test]
    fn no_substitution() {
        let result = render("plain text", &vars(&[]));
        assert_eq!(result, "plain text");
    }

    #[test]
    fn filter_length() {
        let result = render("{{ name | length }}", &vars(&[("name", "hello")]));
        assert_eq!(result, "5");
    }
}
