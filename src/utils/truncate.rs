use unicode_segmentation::UnicodeSegmentation;

pub fn truncate_with_ellipse(s: &str, len: usize, ellipse: &str) -> String {
    let mut lines = s.lines();
    let first_line = lines.next().unwrap_or(s);

    if first_line.graphemes(true).count() <= len {
        if lines.next().is_some() {
            return format!("{first_line}{ellipse}");
        }
        return first_line.to_string();
    }

    s.graphemes(true)
        .take(len)
        .chain(ellipse.graphemes(true))
        .collect()
}
