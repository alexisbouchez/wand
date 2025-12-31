use std::collections::HashMap;

pub fn render(template: &str, vars: &HashMap<String, String>) -> String {
    let result = process_comments(template);
    let result = process_conditionals(&result, vars);
    let result = process_loops(&result, vars);
    process_variables(&result, vars)
}

fn process_comments(template: &str) -> String {
    let mut result = template.to_string();

    while let Some(start) = result.find("{#") {
        if let Some(end) = result[start..].find("#}") {
            let end = start + end + 2;
            result = format!("{}{}", &result[..start], &result[end..]);
        } else {
            break;
        }
    }

    result
}

fn process_conditionals(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    while let Some(if_start) = result.find("{%") {
        let after_tag = &result[if_start + 2..];
        if let Some(tag_end) = after_tag.find("%}") {
            let tag_content = after_tag[..tag_end].trim();

            if tag_content.starts_with("if ") {
                let condition = tag_content.strip_prefix("if ").unwrap().trim();
                let if_end = if_start + 2 + tag_end + 2;

                if let Some(endif_pos) = find_endif(&result[if_end..]) {
                    let block_content = &result[if_end..if_end + endif_pos];
                    let endif_end = if_end + endif_pos + find_tag_len(&result[if_end + endif_pos..], "endif");

                    let (if_block, else_block) = split_if_else(block_content);

                    let output = if eval_condition(condition, vars) {
                        process_conditionals(if_block, vars)
                    } else {
                        process_conditionals(else_block, vars)
                    };

                    result = format!("{}{}{}", &result[..if_start], output, &result[endif_end..]);
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
}

fn find_endif(s: &str) -> Option<usize> {
    let mut depth = 1;
    let mut pos = 0;

    while pos < s.len() {
        if let Some(tag_start) = s[pos..].find("{%") {
            let tag_start = pos + tag_start;
            if let Some(tag_end) = s[tag_start + 2..].find("%}") {
                let tag_content = s[tag_start + 2..tag_start + 2 + tag_end].trim();
                if tag_content.starts_with("if ") {
                    depth += 1;
                } else if tag_content == "endif" {
                    depth -= 1;
                    if depth == 0 {
                        return Some(tag_start);
                    }
                }
                pos = tag_start + 2 + tag_end + 2;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    None
}

fn find_tag_len(s: &str, tag: &str) -> usize {
    if let Some(start) = s.find("{%") {
        if let Some(end) = s[start + 2..].find("%}") {
            let content = s[start + 2..start + 2 + end].trim();
            if content == tag {
                return start + 2 + end + 2;
            }
        }
    }
    0
}

fn split_if_else(block: &str) -> (&str, &str) {
    let mut depth = 0;
    let mut pos = 0;

    while pos < block.len() {
        if let Some(tag_start) = block[pos..].find("{%") {
            let tag_start = pos + tag_start;
            if let Some(tag_end) = block[tag_start + 2..].find("%}") {
                let tag_content = block[tag_start + 2..tag_start + 2 + tag_end].trim();
                if tag_content.starts_with("if ") {
                    depth += 1;
                } else if tag_content == "endif" {
                    depth -= 1;
                } else if tag_content == "else" && depth == 0 {
                    let else_end = tag_start + 2 + tag_end + 2;
                    return (&block[..tag_start], &block[else_end..]);
                }
                pos = tag_start + 2 + tag_end + 2;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    (block, "")
}

fn eval_condition(condition: &str, vars: &HashMap<String, String>) -> bool {
    let condition = condition.trim();

    if let Some((left, right)) = condition.split_once("==") {
        let left = eval_expr(left.trim(), vars);
        let right = eval_expr(right.trim(), vars);
        return left == right;
    }

    if let Some((left, right)) = condition.split_once("!=") {
        let left = eval_expr(left.trim(), vars);
        let right = eval_expr(right.trim(), vars);
        return left != right;
    }

    if condition.starts_with("not ") {
        let inner = condition.strip_prefix("not ").unwrap().trim();
        return !eval_condition(inner, vars);
    }

    // Simple truthy check
    let value = vars.get(condition).cloned().unwrap_or_default();
    !value.is_empty() && value != "false" && value != "0"
}

fn eval_expr(expr: &str, vars: &HashMap<String, String>) -> String {
    let expr = expr.trim();
    if expr.starts_with('"') || expr.starts_with('\'') {
        expr.trim_matches(|c| c == '"' || c == '\'').to_string()
    } else {
        vars.get(expr).cloned().unwrap_or_default()
    }
}

fn process_loops(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    while let Some(for_start) = result.find("{% for ") {
        if let Some(tag_end) = result[for_start + 2..].find("%}") {
            let tag_end = for_start + 2 + tag_end + 2;
            let tag_content = &result[for_start + 7..tag_end - 2].trim();

            if let Some((var_name, items_expr)) = tag_content.split_once(" in ") {
                let var_name = var_name.trim();
                let items_expr = items_expr.trim();

                if let Some(endfor_pos) = find_endfor(&result[tag_end..]) {
                    let block = &result[tag_end..tag_end + endfor_pos];
                    let endfor_end = tag_end + endfor_pos + find_tag_len(&result[tag_end + endfor_pos..], "endfor");

                    let items = get_list_items(items_expr, vars);
                    let mut output = String::new();

                    for item in items {
                        let mut loop_vars = vars.clone();
                        loop_vars.insert(var_name.to_string(), item);
                        output.push_str(&process_variables(block, &loop_vars));
                    }

                    result = format!("{}{}{}", &result[..for_start], output, &result[endfor_end..]);
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
}

fn find_endfor(s: &str) -> Option<usize> {
    let mut depth = 1;
    let mut pos = 0;

    while pos < s.len() {
        if let Some(tag_start) = s[pos..].find("{%") {
            let tag_start = pos + tag_start;
            if let Some(tag_end) = s[tag_start + 2..].find("%}") {
                let tag_content = s[tag_start + 2..tag_start + 2 + tag_end].trim();
                if tag_content.starts_with("for ") {
                    depth += 1;
                } else if tag_content == "endfor" {
                    depth -= 1;
                    if depth == 0 {
                        return Some(tag_start);
                    }
                }
                pos = tag_start + 2 + tag_end + 2;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    None
}

fn get_list_items(expr: &str, vars: &HashMap<String, String>) -> Vec<String> {
    if let Some(items_str) = vars.get(expr) {
        items_str.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec![]
    }
}

fn process_variables(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

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
            if matches!(filter, "basename" | "dirname" | "to_json" | "to_yaml") {
                result = apply_filter_no_args(&result, filter);
            } else {
                result = apply_filter(&result, filter);
            }
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
        "regex_replace" => {
            if let Some((pattern, replacement)) = arg.split_once(',') {
                let pattern = pattern.trim().trim_matches(|c| c == '"' || c == '\'');
                let replacement = replacement.trim().trim_matches(|c| c == '"' || c == '\'');
                if let Ok(re) = regex::Regex::new(pattern) {
                    re.replace_all(value, replacement).to_string()
                } else {
                    value.to_string()
                }
            } else {
                value.to_string()
            }
        }
        "join" => {
            value.split(',').map(|s| s.trim()).collect::<Vec<_>>().join(&arg)
        }
        "split" => {
            value.split(&arg).map(|s| s.to_string()).collect::<Vec<_>>().join(", ")
        }
        _ => value.to_string(),
    }
}

fn apply_filter_no_args(value: &str, filter: &str) -> String {
    match filter {
        "basename" => {
            std::path::Path::new(value)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(value)
                .to_string()
        }
        "dirname" => {
            std::path::Path::new(value)
                .parent()
                .and_then(|s| s.to_str())
                .unwrap_or(value)
                .to_string()
        }
        "to_json" => {
            serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
        }
        "to_yaml" => {
            serde_yaml::to_string(value).unwrap_or_else(|_| value.to_string())
        }
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

    #[test]
    fn if_true() {
        let result = render("{% if enabled %}yes{% endif %}", &vars(&[("enabled", "true")]));
        assert_eq!(result, "yes");
    }

    #[test]
    fn if_false() {
        let result = render("{% if enabled %}yes{% endif %}", &vars(&[("enabled", "false")]));
        assert_eq!(result, "");
    }

    #[test]
    fn if_else_true() {
        let result = render("{% if enabled %}yes{% else %}no{% endif %}", &vars(&[("enabled", "1")]));
        assert_eq!(result, "yes");
    }

    #[test]
    fn if_else_false() {
        let result = render("{% if enabled %}yes{% else %}no{% endif %}", &vars(&[]));
        assert_eq!(result, "no");
    }

    #[test]
    fn if_equals() {
        let result = render("{% if os == \"linux\" %}linux{% endif %}", &vars(&[("os", "linux")]));
        assert_eq!(result, "linux");
    }

    #[test]
    fn if_not_equals() {
        let result = render("{% if os != \"windows\" %}ok{% endif %}", &vars(&[("os", "linux")]));
        assert_eq!(result, "ok");
    }

    #[test]
    fn for_loop() {
        let result = render("{% for item in items %}{{ item }} {% endfor %}", &vars(&[("items", "a,b,c")]));
        assert_eq!(result, "a b c ");
    }

    #[test]
    fn for_loop_empty() {
        let result = render("{% for item in items %}{{ item }}{% endfor %}", &vars(&[]));
        assert_eq!(result, "");
    }

    #[test]
    fn nested_if() {
        let tpl = "{% if a %}{% if b %}both{% endif %}{% endif %}";
        let result = render(tpl, &vars(&[("a", "1"), ("b", "1")]));
        assert_eq!(result, "both");
    }

    #[test]
    fn filter_join() {
        let result = render("{{ items | join(', ') }}", &vars(&[("items", "a,b,c")]));
        assert_eq!(result, "a, b, c");
    }

    #[test]
    fn filter_split() {
        let result = render("{{ path | split('/') }}", &vars(&[("path", "/usr/local/bin")]));
        assert_eq!(result, ", usr, local, bin");
    }

    #[test]
    fn filter_basename() {
        let result = render("{{ path | basename }}", &vars(&[("path", "/usr/local/bin/bash")]));
        assert_eq!(result, "bash");
    }

    #[test]
    fn filter_dirname() {
        let result = render("{{ path | dirname }}", &vars(&[("path", "/usr/local/bin/bash")]));
        assert_eq!(result, "/usr/local/bin");
    }

    #[test]
    fn filter_regex_replace() {
        let result = render("{{ text | regex_replace('\\d+', 'N') }}", &vars(&[("text", "test123abc456")]));
        assert_eq!(result, "testNabcN");
    }

    #[test]
    fn filter_to_json() {
        let result = render("{{ value | to_json }}", &vars(&[("value", "hello")]));
        assert_eq!(result, "\"hello\"");
    }

    #[test]
    fn comment_simple() {
        let result = render("Hello{# comment #} World!", &vars(&[]));
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn comment_multiline() {
        let result = render("Hello{# this is a\nmulti-line comment #} World!", &vars(&[]));
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn comment_with_vars() {
        let result = render("{{ name }}{# comment #} {{ value }}", &vars(&[("name", "foo"), ("value", "bar")]));
        assert_eq!(result, "foo bar");
    }
}
