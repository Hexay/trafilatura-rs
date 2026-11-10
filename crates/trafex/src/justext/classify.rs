//! core.classify_paragraphs and core.revise_paragraph_classification.
use super::paragraph::{Class, Paragraph};
use super::Stoplist;

pub struct Params {
    pub length_low: usize,
    pub length_high: usize,
    pub stopwords_low: f64,
    pub stopwords_high: f64,
    pub max_link_density: f64,
    pub no_headings: bool,
}

/// Context-free classification (`cf_class`, `heading`).
pub fn classify_paragraphs(paragraphs: &mut [Paragraph], stoplist: &Stoplist, p: &Params) {
    for par in paragraphs.iter_mut() {
        let length = par.length;
        let stopword_density = par.stopwords_density(stoplist);
        let link_density = par.links_density();
        par.heading = !p.no_headings && par.is_heading();

        par.cf_class = if link_density > p.max_link_density
            || par.text.contains('\u{a9}')
            || par.text.contains("&copy")
            || par.dom_path.contains("select")
        {
            Class::Bad
        } else if length < p.length_low {
            if par.chars_count_in_links > 0 {
                Class::Bad
            } else {
                Class::Short
            }
        } else if stopword_density >= p.stopwords_high {
            if length > p.length_high {
                Class::Good
            } else {
                Class::NearGood
            }
        } else if stopword_density >= p.stopwords_low {
            Class::NearGood
        } else {
            Class::Bad
        };
    }
}

fn neighbour(mut i: isize, paragraphs: &[Paragraph], ignore_neargood: bool, inc: isize, boundary: isize) -> Class {
    while i + inc != boundary {
        i += inc;
        let c = paragraphs[i as usize].class_type;
        if c == Class::Good || c == Class::Bad || (c == Class::NearGood && !ignore_neargood) {
            return c;
        }
    }
    Class::Bad
}

pub fn get_prev_neighbour(i: usize, paragraphs: &[Paragraph], ignore_neargood: bool) -> Class {
    neighbour(i as isize, paragraphs, ignore_neargood, -1, -1)
}

pub fn get_next_neighbour(i: usize, paragraphs: &[Paragraph], ignore_neargood: bool) -> Class {
    neighbour(i as isize, paragraphs, ignore_neargood, 1, paragraphs.len() as isize)
}

/// True when a 'good' paragraph starts within `max_distance` characters after paragraph `i`.
fn good_follows(i: usize, paragraphs: &[Paragraph], max_distance: usize) -> bool {
    let mut distance = 0;
    for next in &paragraphs[i + 1..] {
        if distance > max_distance {
            break;
        }
        if next.class_type == Class::Good {
            return true;
        }
        distance += next.length;
    }
    false
}

/// Context-sensitive classification (`class_type`); expects `classify_paragraphs` to have run.
pub fn revise_paragraph_classification(paragraphs: &mut [Paragraph], max_heading_distance: usize) {
    // good headings
    for i in 0..paragraphs.len() {
        paragraphs[i].class_type = paragraphs[i].cf_class;
        if !(paragraphs[i].heading && paragraphs[i].class_type == Class::Short) {
            continue;
        }
        if good_follows(i, paragraphs, max_heading_distance) {
            paragraphs[i].class_type = Class::NearGood;
        }
    }

    // classify short
    let mut new_classes = Vec::new();
    for i in 0..paragraphs.len() {
        if paragraphs[i].class_type != Class::Short {
            continue;
        }
        let prev = get_prev_neighbour(i, paragraphs, true);
        let next = get_next_neighbour(i, paragraphs, true);
        let c = if prev == Class::Good && next == Class::Good {
            Class::Good
        } else if prev == Class::Bad && next == Class::Bad {
            Class::Bad
        } else if (prev == Class::Bad && get_prev_neighbour(i, paragraphs, false) == Class::NearGood)
            || (next == Class::Bad && get_next_neighbour(i, paragraphs, false) == Class::NearGood)
        {
            Class::Good
        } else {
            Class::Bad
        };
        new_classes.push((i, c));
    }
    for (i, c) in new_classes {
        paragraphs[i].class_type = c;
    }

    // revise neargood
    for i in 0..paragraphs.len() {
        if paragraphs[i].class_type != Class::NearGood {
            continue;
        }
        let prev = get_prev_neighbour(i, paragraphs, true);
        let next = get_next_neighbour(i, paragraphs, true);
        paragraphs[i].class_type = if (prev, next) == (Class::Bad, Class::Bad) { Class::Bad } else { Class::Good };
    }

    // more good headings
    for i in 0..paragraphs.len() {
        let par = &paragraphs[i];
        if !(par.heading && par.class_type == Class::Bad && par.cf_class != Class::Bad) {
            continue;
        }
        if good_follows(i, paragraphs, max_heading_distance) {
            paragraphs[i].class_type = Class::Good;
        }
    }
}
