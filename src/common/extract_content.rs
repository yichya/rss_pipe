use nanohtml2text::html2text;
use regex::Regex;

const SIMILAR_RATIO: f32 = 0.9;

fn levenshtein_distance(str1: &str, str2: &str) -> usize {
    if str1 == str2 {
        return 0;
    }

    let s1_len = str1.chars().count();
    let s2_len = str2.chars().count();

    if s1_len == 0 {
        return s2_len;
    }
    if s2_len == 0 {
        return s1_len;
    }

    let mut v0: Vec<usize> = vec![0; s2_len + 1];
    for i in 0..s2_len + 1 {
        v0[i] = i
    }

    let mut v1: Vec<usize> = vec![0; s2_len + 1];
    for i in 0..s1_len {
        v1[0] = i + 1;
        for j in 0..s2_len {
            let cost = if str1.chars().nth(i) == str2.chars().nth(j) {
                0
            } else {
                1
            };
            let v = [v1[j] + 1, v0[j + 1] + 1, v0[j] + cost];
            v1[j + 1] = *v.iter().min().unwrap();
        }
        for j in 0..s2_len + 1 {
            v0[j] = v1[j]
        }
    }

    v1[s2_len]
}

fn clean_str(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|x| if x.is_alphabetic() || x.is_numeric() { x } else { ' ' })
        .collect();
    cleaned.trim_matches(' ').to_owned()
}

fn ratio(s1: &str, s2: &str) -> f32 {
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    let l = s1.chars().count() + s2.chars().count();
    let dist = levenshtein_distance(clean_str(s1).as_str(), clean_str(s2).as_str());
    1.0 - (dist as f32 / l as f32)
}

fn partial_ratio(s1: &str, s2: &str) -> f32 {
    let (min_str, max_str) = if s1.chars().count() < s2.chars().count() {
        (s1, s2)
    } else {
        (s2, s1)
    };
    let mut best_ratio: f32 = 0.0;

    for i in 0..max_str.chars().count() - min_str.chars().count() + 1 {
        let current_ratio = ratio(
            min_str,
            max_str
                .chars()
                .skip(i)
                .take(min_str.chars().count())
                .collect::<String>()
                .as_str(),
        );
        if current_ratio > best_ratio {
            best_ratio = current_ratio;
        }
    }

    best_ratio
}

fn clean_html(content: &str) -> String {
    let content_br = content.replace("\n", "\n<br/>");
    let content_no_link = content_br.replace("<a href", "<div ignore").replace("</a>", "</div>");
    Regex::new("<img alt=['\"]([^'\"]*)['\"]")
        .unwrap()
        .replace_all(&content_no_link, "${1}<img")
        .to_string()
}

fn similar(title: &str, content: &str) -> bool {
    let content_no_new_line = content.replace("\r\n", "");
    partial_ratio(title, &content_no_new_line) > SIMILAR_RATIO
}

pub fn extract_content(title: &str, content: &str, max_size: usize) -> String {
    let content_cleaned = clean_html(content);
    let content_extracted = html2text(content_cleaned.as_str());
    let content_final = if similar(title, content_extracted.as_str()) {
        content_extracted
    } else {
        format!("{title}\r\n{content_extracted}")
    }
    .trim()
    .to_owned();
    if content_final.chars().count() < max_size {
        content_final
    } else {
        let content_trimmed: String = content_final.chars().take(max_size).collect();
        format!("{content_trimmed}...")
    }
}
