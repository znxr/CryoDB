pub(crate) fn max_chars_for_width(width: f32) -> usize {
    let estimate = (width / 8.0).floor() as usize;
    estimate.max(8)
}

pub(crate) fn truncate_with_ellipsis(input: &str, max_chars: usize) -> (String, bool) {
    if input.chars().count() <= max_chars {
        return (input.to_string(), false);
    }

    let mut output = String::new();
    for (index, ch) in input.chars().enumerate() {
        if index + 1 >= max_chars {
            break;
        }
        output.push(ch);
    }
    output.push_str("...");
    (output, true)
}

pub(crate) fn compact_inline_preview(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), false);
    }

    let mut output = String::new();
    let mut last_space = false;
    let mut count = 0usize;

    for ch in input.chars() {
        let mapped = match ch {
            '\n' | '\r' | '\t' => ' ',
            _ if ch.is_control() => ' ',
            _ => ch,
        };

        if mapped == ' ' {
            if output.is_empty() || last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }

        if count + 1 >= max_chars {
            output.push_str("...");
            return (output, true);
        }

        output.push(mapped);
        count += 1;
    }

    (output, false)
}
