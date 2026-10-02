use iced::{Color, widget::text::Span};

#[derive(Debug, Clone)]
pub(crate) struct FuzzyMatch {
    pub(crate) score: i32,
    pub(crate) indices: Vec<usize>,
}

pub(crate) fn normalize_omni_lookup(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

pub(crate) fn fuzzy_match(haystack: &str, needle: &str) -> Option<FuzzyMatch> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            indices: Vec::new(),
        });
    }

    let needle_lower: Vec<char> = needle.chars().map(|ch| ch.to_ascii_lowercase()).collect();
    if needle_lower.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            indices: Vec::new(),
        });
    }

    let mut indices = Vec::with_capacity(needle_lower.len());
    let mut haystack_len = 0_usize;
    let mut needle_index = 0_usize;
    for ch in haystack.chars() {
        if needle_index < needle_lower.len()
            && ch.to_ascii_lowercase() == needle_lower[needle_index]
        {
            indices.push(haystack_len);
            needle_index += 1;
        }
        haystack_len += 1;
    }
    if needle_index != needle_lower.len() {
        return None;
    }

    let span = indices
        .last()
        .copied()
        .unwrap_or(0)
        .saturating_sub(indices.first().copied().unwrap_or(0));
    let consecutive_bonus = indices
        .windows(2)
        .filter(|pair| pair[1] == pair[0] + 1)
        .count() as i32
        * 8;
    let prefix_bonus = if indices.first().copied() == Some(0) {
        25
    } else {
        0
    };
    let compact_bonus = (40_i32 - span as i32).max(0);
    let score = 100 + consecutive_bonus + prefix_bonus + compact_bonus - haystack_len as i32;

    Some(FuzzyMatch { score, indices })
}

pub(crate) fn highlighted_spans(
    value: &str,
    indices: &[usize],
    base_color: Color,
    highlight_color: Color,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut segment = String::new();
    let mut segment_highlighted = false;
    let mut has_segment = false;
    let mut highlighted_position = 0_usize;
    let mut next_highlight = indices.get(highlighted_position).copied();

    for (index, ch) in value.chars().enumerate() {
        let is_highlighted = next_highlight.is_some_and(|target| target == index);
        if is_highlighted {
            highlighted_position += 1;
            next_highlight = indices.get(highlighted_position).copied();
        }
        if !has_segment {
            segment_highlighted = is_highlighted;
            has_segment = true;
        } else if is_highlighted != segment_highlighted {
            let span = if segment_highlighted {
                Span::new(segment.clone()).color(highlight_color)
            } else {
                Span::new(segment.clone()).color(base_color)
            };
            spans.push(span);
            segment.clear();
            segment_highlighted = is_highlighted;
        }
        segment.push(ch);
    }

    if !segment.is_empty() {
        let span = if segment_highlighted {
            Span::new(segment).color(highlight_color)
        } else {
            Span::new(segment).color(base_color)
        };
        spans.push(span);
    }

    if spans.is_empty() {
        spans.push(Span::new(String::new()));
    }

    spans
}
