use aidoku::{alloc::string::String, prelude::format};

pub const BASE_URL: &str = "https://rokaricomics.com";

/// Build a full URL from a path segment like "manga/slug/" or "adopting-...-chapter-1".
pub fn build_url_from_path(path: &str) -> String {
	let trimmed = path.trim_matches('/');
	format!("{}/{}", BASE_URL, trimmed)
}

/// Extract a series key from any URL pointing to a manga page.
/// The key is the path part without leading/trailing slashes, e.g.
///   "manga/7387837087-adopting-the-male-protagonist-changed-the-genre".
pub fn get_series_id_from_url(url: &str) -> Option<String> {
	let lower = url.to_lowercase();
	let marker = "/manga/";
	let pos = lower.find(marker)?;
	let mut rest = &url[pos + marker.len() - 6..]; // include "manga/" in key
	if let Some(end) = rest.find(['?', '#']) { rest = &rest[..end]; }
	let key = rest.trim_matches('/');
	if key.is_empty() { None } else { Some(String::from(key)) }
}

/// Extract a chapter key from any URL pointing to a chapter page.
/// The key is simply the path without leading/trailing slashes, e.g.
///   "adopting-the-male-protagonist-changed-the-genre-chapter-1" or
///   "7387-...-adopting-the-...-chapter-25".
pub fn get_chapter_id_from_url(url: &str) -> Option<String> {
	// Strip scheme and domain, keep only path
	let mut start = 0usize;
	if let Some(pos) = url.find("//") {
		if let Some(slash) = url[pos + 2..].find('/') {
			start = pos + 2 + slash + 1;
		}
	}
	let path = &url[start..];
	let mut p = path.trim_matches('/');
	if let Some(end) = p.find(['?', '#']) { p = &p[..end]; }
	if p.is_empty() { None } else { Some(String::from(p)) }
}
