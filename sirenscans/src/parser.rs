use aidoku::{
    prelude::*,
    alloc::{ string::{String, ToString}, vec, vec::Vec },
    imports::net::{HttpMethod, Request},
    Viewer,Manga, MangaPageResult, Chapter, Page, PageContent, MangaStatus, ContentRating, Result,
};
use aidoku::imports::defaults::defaults_get;
use crate::helper::{build_chapter_url, build_series_url, get_chapter_id_from_url, BASE_URL};

// Minimal percent-decoder for URLs (no_std friendly)
fn percent_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            let v1 = match h1 { b'0'..=b'9' => h1 - b'0', b'a'..=b'f' => 10 + h1 - b'a', b'A'..=b'F' => 10 + h1 - b'A', _ => 255 };
            let v2 = match h2 { b'0'..=b'9' => h2 - b'0', b'a'..=b'f' => 10 + h2 - b'a', b'A'..=b'F' => 10 + h2 - b'A', _ => 255 };
            if v1 != 255 && v2 != 255 {
                out.push((v1 * 16 + v2) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn normalize_cover_url(u: &str) -> Option<String> {
    let mut s = u.trim().to_string();
    if s.is_empty() { return None; }
    // Decode %xx and HTML entity ampersands already handled earlier
    s = percent_decode(&s);
    // Protocol-relative
    if s.starts_with("//") { s = format!("https:{}", s); }
    // Relative path
    if s.starts_with('/') && !s.starts_with("/http") {
        s = format!("{}{}", BASE_URL, s);
    }
    // Basic sanity: must look like an image URL
    if s.starts_with("http://") || s.starts_with("https://") { Some(s) } else { None }
}

pub fn parse_manga_list(page: i32) -> Result<MangaPageResult> {
    // Always use the series listing to avoid homepage duplicates and parse its known structure
    let url = if page <= 1 { format!("{}/series", BASE_URL) } else { format!("{}/series?page={}", BASE_URL, page) };

    let html = match Request::new(url.clone(), HttpMethod::Get) {
        Ok(req) => match req.html() {
            Ok(h) => h,
            Err(_e) => {
                println!("failed to load page: {}", url);
                return Ok(MangaPageResult { entries: Vec::new(), has_next_page: false });
            }
        },
        Err(_e) => {
            println!("failed to load page: {}", url);
            return Ok(MangaPageResult { entries: Vec::new(), has_next_page: false });
        }
    };

    let mut entries: Vec<Manga> = Vec::new();
    let use_series_fallback = false; // disable costly per-item fetch to avoid slow/no-load
    let mut _seen: i32 = 0;
    let mut _pushed: i32 = 0;

    // Target the container the site uses on /series
    let container = html.select_first("#searched_series_page");
    let buttons = container.as_ref().and_then(|c| c.select("button[id][title]"));
    if let Some(mut list) = buttons {
    for btn in &mut list {
        _seen += 1;
        let key = btn.attr("id").unwrap_or_default();
        if key.is_empty() { continue; }
        let title_guess = btn.attr("title").unwrap_or_else(|| key.clone());

        // Find the anchor that links to the series page within the first grid div inside the button
        let anchor = btn
            .select_first("div[class*=grid] a[href*=/series/]")
            .or_else(|| btn.select_first("a[href*=/series/]"));
        let href = anchor.as_ref().and_then(|a| a.attr("href")).unwrap_or_default();
        if href.contains("/series?") || href.contains("/?series=") { continue; }

        // Extract cover from the inner styled div inside the anchor
        let mut cover = {
            let styled = anchor.as_ref().and_then(|a| a.select_first("[style*=background-image]"))
                .and_then(|e| e.attr("style"));
            let style_bg = styled.and_then(|s| {
                let s_lower = s.as_str();
                let start = s_lower.find("url(");
                let end = s_lower.rfind(")");
                match (start, end) {
                    (Some(si), Some(ei)) if ei > si + 4 => {
                        let mut v = String::from(&s_lower[si + 4..ei]);
                        if v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2 { v = v[1..v.len()-1].into(); }
                        if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 { v = v[1..v.len()-1].into(); }
                        let mut url_str = v.replace("&amp;", "&");
                        if let Some(qi) = url_str.find('?') {
                            let (base, query) = url_str.split_at(qi);
                            let base_owned = base.to_string();
                            let query_owned = query[1..].to_string();
                            let mut kept: Vec<String> = Vec::new();
                            for part in query_owned.split('&') {
                                let p = part.trim();
                                if p.is_empty() { continue; }
                                let lp = p.to_ascii_lowercase();
                                if lp.starts_with("w=") || lp.starts_with("h=") || lp.starts_with("width=") || lp.starts_with("height=") { continue; }
                                if let Some(pos) = p.find("url=") {
                                    let val = &p[pos+4..];
                                    if !val.starts_with("http://") && !val.starts_with("https://") { kept.push(format!("url=https://{}", val)); continue; }
                                }
                                kept.push(p.to_string());
                            }
                            if kept.is_empty() { url_str = base_owned; }
                            else { let joined = kept.join("&"); url_str = base_owned; url_str.push('?'); url_str.push_str(&joined); }
                        }
                        normalize_cover_url(&url_str)
                    }
                    _ => None,
                }
            }).unwrap_or_default();
            if !style_bg.is_empty() {
                if style_bg.contains("wsrv.nl") {
                    if let Some(pos) = style_bg.find("url=") {
                        let mut val = &style_bg[pos+4..];
                        if let Some(end) = val.find('&') { val = &val[..end]; }
                        let out = normalize_cover_url(val);
                        out
                    } else { Some(style_bg) }
                } else { Some(style_bg) }
            } else { None }
        };
        // If still no cover, fetch series page and try meta tags or images
        if cover.is_none() && use_series_fallback {
            let series_url = build_series_url(&key);
            if let Ok(series_html) = Request::new(series_url.clone(), HttpMethod::Get).and_then(|r| r.html()) {
                let og = series_html.select_first("meta[property=og:image], meta[name=og:image]")
                    .and_then(|e| e.attr("content")).unwrap_or_default();
                if !og.is_empty() {
                    let norm = normalize_cover_url(&og);
                    cover = norm;
                } else {
                    let link_img = series_html.select_first("link[rel=image_src], link[rel=apple-touch-icon], link[rel=icon]")
                        .and_then(|e| e.attr("href")).unwrap_or_default();
                    if !link_img.is_empty() {
                        let norm = normalize_cover_url(&link_img);
                        cover = norm;
                    } else {
                        let img = series_html.select_first(".cover img, .series-cover img, .manga-cover img, img");
                        let src = img.as_ref().and_then(|e| e.attr("abs:src")).unwrap_or_default();
                        let c = if src.is_empty() { img.as_ref().and_then(|e| e.attr("src")).unwrap_or_default() } else { src };
                        if !c.is_empty() {
                            let norm = normalize_cover_url(&c);
                            cover = norm;
                        }
                    }
                }
            }
        }
        // no debug logs
        // avoid duplicates if the page lists the same series multiple times
        if let Some(existing) = entries.iter_mut().find(|m| m.key == key) {
            if existing.cover.is_none() {
                if let Some(c) = cover.clone() {
                    existing.cover = Some(c);
                }
            }
            continue;
        }
        let url = build_series_url(&key);
        let title = title_guess;
        // no debug logs
        entries.push(Manga { key, title, cover, url: Some(url), ..Default::default() });
        _pushed += 1;
    }
    }

    // Series listing pagination is present even on page 1
    let has_next_page = html
        .select("a[rel=next], a:matchesOwn(Next), button:matchesOwn(Next)")
        .map(|l| !l.is_empty())
        .unwrap_or(false);
    Ok(MangaPageResult { entries, has_next_page })
}

pub fn parse_manga_details(id: String) -> Result<Manga> {
    let url = build_series_url(&id);
    let html = match Request::new(url.clone(), HttpMethod::Get) {
        Ok(req) => match req.html() {
            Ok(h) => h,
            Err(e) => { println!("failed to load page: {}", url); return Err(aidoku::AidokuError::RequestError(e)); }
        },
        Err(e) => { println!("failed to load page: {}", url); return Err(aidoku::AidokuError::RequestError(e)); }
    };
    // Prefer the body, ignoring header/panel/script regions implicitly by scoping to content divs
    let wrapper = html.select_first("body, main, .container, .series, .series-page");

    // COVER: prefer elements with --photoURL:url(...), then fallback to background-image:url(...)
    let cover = {
        // First try CSS variable form explicitly to avoid picking chapter thumbnails
        let styled = wrapper.as_ref()
            .and_then(|w| w.select_first("[style*='--photoURL:url(']"))
            .or_else(|| wrapper.as_ref().and_then(|w| w.select_first("div[style*=background-image], [style*=background-image]")))
            .and_then(|e| e.attr("style"));
        let from_style = styled.and_then(|s| {
            let s_lower = s.as_str();
            // Extract the inner value of url(...)
            let (start, end) = (s_lower.find("url("), s_lower.rfind(")"));
            match (start, end) {
                (Some(si), Some(ei)) if ei > si + 4 => {
                    let mut v = String::from(&s_lower[si + 4..ei]);
                    if v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2 { v = v[1..v.len()-1].into(); }
                    if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 { v = v[1..v.len()-1].into(); }
                    let mut url_str = v.replace("&amp;", "&");
                    // Strip problematic sizing params
                    if let Some(qi) = url_str.find('?') {
                        let (base, query) = url_str.split_at(qi);
                        let base_owned = base.to_string();
                        let query_owned = query[1..].to_string();
                        let mut kept: Vec<String> = Vec::new();
                        for part in query_owned.split('&') {
                            let p = part.trim(); if p.is_empty() { continue; }
                            let lp = p.to_ascii_lowercase();
                            if lp.starts_with("w=") || lp.starts_with("h=") || lp.starts_with("width=") || lp.starts_with("height=") { continue; }
                            // Normalize wsrv.nl inner url
                            if let Some(pos) = p.find("url=") {
                                let val = &p[pos+4..];
                                if !val.starts_with("http://") && !val.starts_with("https://") { kept.push(format!("url=https://{}", val)); continue; }
                            }
                            kept.push(p.to_string());
                        }
                        if kept.is_empty() { url_str = base_owned; }
                        else { let joined = kept.join("&"); url_str = base_owned; url_str.push('?'); url_str.push_str(&joined); }
                    }
                    // If wrapped by wsrv.nl, unwrap inner url= when present
                    if url_str.contains("wsrv.nl") {
                        if let Some(pos) = url_str.find("url=") {
                            let mut val = &url_str[pos+4..];
                            if let Some(end) = val.find('&') { val = &val[..end]; }
                            return normalize_cover_url(val);
                        }
                    }
                    normalize_cover_url(&url_str)
                }
                _ => None,
            }
        });
        if from_style.is_some() { from_style }
        else {
            // Fallback to common <img> locations
            let c = wrapper.as_ref()
                .and_then(|w| w.select_first("img[alt*=cover], img.cover, .poster img, img"))
                .and_then(|e| e.attr("abs:src"))
                .unwrap_or_default();
            let c2 = if c.is_empty() {
                wrapper.as_ref()
                    .and_then(|w| w.select_first("img"))
                    .and_then(|e| e.attr("src"))
                    .unwrap_or_default()
            } else { c };
            if c2.is_empty() { None } else { normalize_cover_url(&c2) }
        }
    };
    let title = {
        let t = wrapper.as_ref()
            .and_then(|w| w.select("h1, h2, .title, .name"))
            .and_then(|l| l.text())
            .unwrap_or_default();
        if t.is_empty() { id.clone() } else { t }
    };
    // TAGS: list of <a> under the tags row beneath the title
    let mut tags: Vec<String> = Vec::new();
    // Anchor the search to the title block to avoid grabbing sitewide tags
    let title_el = wrapper.as_ref().and_then(|w| w.select_first("h1, h2, .title, .name"));
    let scoped_container = title_el
        .as_ref()
        .and_then(|t| t.parent())
        .or_else(|| title_el.as_ref().and_then(|t| t.parent().and_then(|p| p.parent())));
    if let Some(container) = scoped_container {
        if let Some(mut els) = container.select(".flex.flex-wrap a[href*='/series?'], .flex.flex-wrap a[href*='/series/?']") {
            for g in &mut els {
                let t = g.text().unwrap_or_default();
                if !t.is_empty() { tags.push(t); }
            }
        }
    }
    // Fallback: narrow page-wide search but still dedupe and cap size
    if tags.is_empty() {
        if let Some(mut els) = wrapper.as_ref().and_then(|w| w.select(".flex.flex-wrap a[href*='/series?'], .flex.flex-wrap a[href*='/series/?']")) {
            for g in &mut els {
                let t = g.text().unwrap_or_default();
                if !t.is_empty() && !tags.iter().any(|x| x == &t) { tags.push(t); }
                if tags.len() >= 8 { break; }
            }
        }
    }

    // SYNOPSIS: inside an expandable content container
    let description = {
        let d = wrapper.as_ref()
            .and_then(|w| w.select("[id*=expand][id*=content], #expand-content, #expandContent, .expand-content, .synopsis p, .summary p"))
            .and_then(|l| l.text())
            .unwrap_or_default();
        if d.is_empty() { None } else { Some(d) }
    };
    let author = {
        let a = wrapper.as_ref()
            .and_then(|w| w.select("*:matchesOwn(Author) + *, .author, .authors a"))
            .and_then(|l| l.text())
            .unwrap_or_default();
        if a.is_empty() { None } else { Some(vec![a]) }
    };
    let artist = {
        let a = wrapper.as_ref()
            .and_then(|w| w.select("*:matchesOwn(Artist) + *, .artist, .artists a"))
            .and_then(|l| l.text())
            .unwrap_or_default();
        if a.is_empty() { None } else { Some(vec![a]) }
    };

    // tags already collected above

    let status = {
        let s = wrapper.as_ref()
            .and_then(|w| w.select("*:matchesOwn(Status) + *, .status, .metadata .status"))
            .and_then(|l| l.text())
            .unwrap_or_default()
            .to_lowercase();
        if s.contains("ongoing") { MangaStatus::Ongoing }
        else if s.contains("complete") { MangaStatus::Completed }
        else if s.contains("hiatus") { MangaStatus::Hiatus }
        else if s.contains("drop") { MangaStatus::Cancelled }
        else { MangaStatus::Unknown }
    };

    let mut content_rating = ContentRating::Safe;
    for c in &tags { let lc = c.to_lowercase(); if lc.contains("adult") || lc.contains("ecchi") { content_rating = ContentRating::Suggestive; } }

    Ok(Manga {
        key: id,
        title,
        cover,
        url: Some(url),
        description,
        authors: author,
        artists: artist,
        tags: if tags.is_empty() { None } else { Some(tags) },
        status,
        content_rating,
		viewer: Viewer::Webtoon,
        ..Default::default()
    })
}

pub fn parse_chapter_list(id: String) -> Result<Vec<Chapter>> {
    let url = build_series_url(&id);
    let html = match Request::new(url.clone(), HttpMethod::Get) {
        Ok(req) => match req.html() {
            Ok(h) => h,
            Err(e) => { println!("failed to load page: {}", url); return Err(aidoku::AidokuError::RequestError(e)); }
        },
        Err(e) => { println!("failed to load page: {}", url); return Err(aidoku::AidokuError::RequestError(e)); }
    };
    let mut chapters: Vec<Chapter> = Vec::new();

    // Limit to the real chapters grid to avoid Start/New buttons above
    let scope = html.select_first("#chapters");
    if let Some(mut list) = scope.as_ref().and_then(|c| c.select("a[href*=/chapter/]")) {
    for node in &mut list {
        let href = node.attr("href").unwrap_or_default();
        if !href.contains("/chapter/") { continue; }
        let chapter_id = get_chapter_id_from_url(&href).unwrap_or_default();
        if chapter_id.is_empty() { continue; }
        let url = build_chapter_url(&chapter_id);

        // Skip chapters marked with a bold span inside the link (paid/locked indicator)
        if node.select_first("span.font-bold").is_some() { continue; }

        // Prefer clean chapter title from attributes
        let mut title_text = node.attr("title").unwrap_or_default();
        if title_text.is_empty() { title_text = node.attr("alt").unwrap_or_default(); }
        if title_text.is_empty() {
            // Fallback to visible text; strip badge/time/cost noise
            let raw = node.text().unwrap_or_default();
            let mut cleaned = raw.replace("New", "");
            // Remove common time-ago phrases
            for kw in ["hours ago", "hour ago", "days ago", "day ago", "minutes ago", "minute ago", "weeks ago", "week ago"] {
                cleaned = cleaned.replace(kw, "");
            }
            // Collapse whitespace
            title_text = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        let chapter_num = title_text
            .replace("Chapter", "").replace("chapter", "").trim()
            .parse::<f32>().ok();

        // Date parsing helpers are not available; leave as None by setting to 0
        let _date_uploaded_str = node.parent()
            .and_then(|p| p.select("time, .date"))
            .and_then(|l| l.text())
            .unwrap_or_default();
        let date_uploaded = 0;

        chapters.push(Chapter {
            key: chapter_id,
            title: Some(String::from(title_text.trim())),
            chapter_number: chapter_num,
            date_uploaded: if date_uploaded == 0 { None } else { Some(date_uploaded) },
            url: Some(url),
            ..Default::default()
        });
    }
    }

    Ok(chapters)
}

pub fn parse_page_list(chapter_id: String) -> Result<Vec<Page>> {
    let url = build_chapter_url(&chapter_id);
    println!("parse_page_list: {}", url);
    let html = match Request::new(url.clone(), HttpMethod::Get) {
        Ok(req) => match req.html() {
            Ok(h) => h,
            Err(e) => { println!("failed to load page: {}", url); return Err(aidoku::AidokuError::RequestError(e)); }
        },
        Err(e) => { println!("failed to load page: {}", url); return Err(aidoku::AidokuError::RequestError(e)); }
    };

    let mut pages: Vec<Page> = Vec::new();
    let mut seen_urls: Vec<String> = Vec::new();

    let _preferred_attr = defaults_get::<String>("imageAttr")
        .unwrap_or_else(|| String::from("auto"));
    // If the chapter is paywalled or the container is absent, do not try other fallbacks
    if html.select_first("#pages").is_none() {
        return Ok(pages);
    }
    // STRICT path: only grab <img> tags from the div with id="pages" and return
    if let Some(pages_div) = html.select_first("#pages") {
        // Diagnostics: count imgs under #pages and log first 3 raw srcs
        let mut total_imgs = 0;
        if let Some(mut els) = pages_div.select("img") { for _ in &mut els { total_imgs += 1; } }
        let mut logged = 0;
        if let Some(mut els) = pages_div.select("img") {
            for e in &mut els {
                if logged >= 3 { break; }
                let s = e.attr("src").unwrap_or_default();
                println!("#pages img[{}] raw src={}", logged, s);
                logged += 1;
            }
        }
        // Minimal logic: prefer numbered images (read src), else all imgs (read src)
        let mut ordered: Vec<(i32, String)> = Vec::new();
        if let Some(mut imgs) = pages_div.select("img.myImage[count], img[count]") {
            for img in &mut imgs {
                let mut src = img.attr("src").unwrap_or_default();
                let mut lc = src.to_lowercase();
                // If empty or placeholder/SVG, construct from uid
                if src.is_empty() || lc.ends_with(".svg") || lc.contains("/assets/") || lc.contains("placeholder") {
                    let uid = img.attr("uid").unwrap_or_default();
                    if !uid.is_empty() {
                        src = format!("https://cdn.meowing.org/uploads/{}", uid);
                        lc = src.to_lowercase();
                    }
                }
                if src.is_empty() { continue; }
                if lc.ends_with(".svg") || lc.contains("/assets/") || lc.contains("placeholder") { continue; }
                let idx = img.attr("count").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                ordered.push((idx, src));
            }
        }
        if ordered.is_empty() {
            if let Some(mut imgs) = pages_div.select("img.myImage, img") {
                let mut i: i32 = 0;
                for img in &mut imgs {
                    let mut src = img.attr("src").unwrap_or_default();
                    let mut lc = src.to_lowercase();
                    if src.is_empty() || lc.ends_with(".svg") || lc.contains("/assets/") || lc.contains("placeholder") {
                        let uid = img.attr("uid").unwrap_or_default();
                        if !uid.is_empty() {
                            src = format!("https://cdn.meowing.org/uploads/{}", uid);
                            lc = src.to_lowercase();
                        }
                    }
                    if src.is_empty() { continue; }
                    if lc.ends_with(".svg") || lc.contains("/assets/") || lc.contains("placeholder") { continue; }
                    ordered.push((i, src));
                    i += 1;
                }
            }
        }
        if !ordered.is_empty() {
            ordered.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, mut src) in ordered {
                // Proxy non-site CDN URLs through wsrv.nl to avoid hotlink blocks
                let lc = src.to_lowercase();
                if !lc.contains("sirenscans.com") && !lc.contains("wsrv.nl") {
                    let safe = src.replace("&", "%26");
                    src = format!("https://wsrv.nl/?url={}", safe);
                }
                println!("loading image: {}", src);
                pages.push(Page { content: PageContent::url(src), ..Default::default() });
            }
        } else {
            println!("#pages had {} <img>, but none passed filters", total_imgs);
        }
        return Ok(pages);
    }
    // Prefer the explicit pages container used by Siren Scans
    if let Some(pages_div) = html.select_first("#pages") {
        // Diagnostics: presence flags (ElementList has no len())
        let has_imgs = pages_div.select("img").map(|_| 1).unwrap_or(0);
        let has_img_count = pages_div.select("img[count]").map(|_| 1).unwrap_or(0);
        let has_pictures = pages_div.select("picture").map(|_| 1).unwrap_or(0);
        let has_sources = pages_div.select("source[srcset]").map(|_| 1).unwrap_or(0);
        println!("#pages diagnostics: img?={} img[count]?={} picture?={} source[srcset]?={}", has_imgs, has_img_count, has_pictures, has_sources);
        // Log attributes of the first few imgs/sources to diagnose
        if has_imgs == 1 {
            if let Some(mut probe_imgs) = pages_div.select("img") {
                let mut i = 0;
                for im in &mut probe_imgs {
                    if i >= 3 { break; }
                    let s = im.attr("src").unwrap_or_default();
                    let asrc = im.attr("abs:src").unwrap_or_default();
                    let ds = im.attr("data-src").unwrap_or_default();
                    let dorig = im.attr("data-original").unwrap_or_default();
                    let dls = im.attr("data-lazy-src").unwrap_or_default();
                    let du = im.attr("data-url").unwrap_or_default();
                    let ss = im.attr("srcset").unwrap_or_default();
                    let cnt = im.attr("count").unwrap_or_default();
                    let dcnt = im.attr("data-count").unwrap_or_default();
                    println!("#pages img[{}]: src={} abs:src={} data-src={} data-original={} data-lazy-src={} data-url={} srcset={} count={} data-count={}", i, s, asrc, ds, dorig, dls, du, ss, cnt, dcnt);
                    i += 1;
                }
            }
        }
        if has_pictures == 1 || has_sources == 1 {
            if let Some(mut probe_sources) = pages_div.select("picture source[srcset], source[srcset]") {
                let mut i = 0;
                for s in &mut probe_sources {
                    if i >= 3 { break; }
                    let ss = s.attr("srcset").unwrap_or_default();
                    println!("#pages source[{}]: srcset={}", i, ss);
                    i += 1;
                }
            }
        }
        // Collect with ordering based on optional/nested `count` attribute
        let mut ordered: Vec<(i32, String)> = Vec::new();
        let mut fallback_idx: i32 = 0;

        // Case 1: <img count> (or data-count) anywhere under #pages
        if let Some(mut imgs) = pages_div.select("img[count], img[data-count]") {
            for img in &mut imgs {
                // Skip donate/support images by alt text if any slipped in
                let alt_lc = img.attr("alt").unwrap_or_default().to_lowercase();
                if alt_lc.contains("donate") || alt_lc.contains("support") { continue; }
                // Try a set of attributes, preferring raster images and ignoring SVG placeholders
                let mut cand: Vec<String> = Vec::new();
                // Explicit preference: src should already be the real URL per site context
                let s_src = img.attr("abs:src").unwrap_or_default(); if !s_src.is_empty() { cand.push(s_src); }
                let s1 = img.attr("data-src").unwrap_or_default(); if !s1.is_empty() { cand.push(s1); }
                let s2 = img.attr("data-original").unwrap_or_default(); if !s2.is_empty() { cand.push(s2); }
                let s3 = img.attr("data-lazy-src").unwrap_or_default(); if !s3.is_empty() { cand.push(s3); }
                let s4 = img.attr("data-url").unwrap_or_default(); if !s4.is_empty() { cand.push(s4); }
                // If img has a srcset, parse it too
                let sset = img.attr("srcset").unwrap_or_default(); if !sset.is_empty() { cand.push(sset); }

                // Pick first non-SVG, likely raster
                let mut chosen = String::new();
                for c in cand {
                    let lc = c.to_lowercase();
                    if lc.contains(",") && lc.contains(" ") {
                        // Likely a srcset list; take the first valid URL
                        for part in c.split(',') {
                            let u = part.trim().split_whitespace().next().unwrap_or("");
                            let ulc = u.to_lowercase();
                            if ulc.is_empty() { continue; }
                            if ulc.ends_with(".svg") || ulc.contains("placeholder") || ulc.contains("iconify.design") || ulc.contains("calendar-badge") || ulc.contains("senkoheart") { continue; }
                            // Accept common raster formats, else accept generic http(s)
                            if ulc.ends_with(".jpg") || ulc.ends_with(".jpeg") || ulc.ends_with(".png") || ulc.ends_with(".webp") || ulc.ends_with(".avif") || ulc.starts_with("http://") || ulc.starts_with("https://") || ulc.starts_with("/") || ulc.starts_with("//") { chosen = u.to_string(); break; }
                        }
                        if !chosen.is_empty() { break; }
                        continue;
                    }
                    if lc.ends_with(".svg") || lc.contains("placeholder") || lc.contains("iconify.design") || lc.contains("calendar-badge") || lc.contains("senkoheart") { continue; }
                    // Accept raster or any non-SVG http(s)/relative URL
                    if lc.ends_with(".jpg") || lc.ends_with(".jpeg") || lc.ends_with(".png") || lc.ends_with(".webp") || lc.ends_with(".avif") || lc.starts_with("http://") || lc.starts_with("https://") || lc.starts_with("/") || lc.starts_with("//") { chosen = c; break; }
                }
                if chosen.is_empty() { continue; }
                // Normalize potential relative/protocol-relative URLs
                if let Some(norm) = normalize_cover_url(&chosen) { chosen = norm; }
                // Determine index from `count` attribute (required by selector)
                let idx = match img.attr("count").or_else(|| img.attr("data-count")).and_then(|v| v.parse::<i32>().ok()) {
                    Some(n) => n,
                    None => { let i = fallback_idx; fallback_idx += 1; i }
                };
                ordered.push((idx, chosen));
            }
        }

        // Case 2: <picture> with <source srcset> and nested <img count>
        if let Some(mut pictures) = pages_div.select("> picture") {
            for pic in &mut pictures {
                let count_from_img = pic.select_first("img[count]")
                    .and_then(|i| i.attr("count").and_then(|v| v.parse::<i32>().ok()));
                // Gather candidate srcset values
                let mut cand: Vec<String> = Vec::new();
                if let Some(mut sources) = pic.select("source[srcset]") {
                    for s in &mut sources {
                        let srcset = s.attr("srcset").unwrap_or_default();
                        if !srcset.is_empty() { cand.push(srcset); }
                    }
                }
                // Parse srcset entries to URLs
                let mut chosen = String::new();
                for entry in cand {
                    // srcset: "url1 1x, url2 2x" or "url 800w"
                    for part in entry.split(',') {
                        let u = part.trim().split_whitespace().next().unwrap_or("");
                        if u.is_empty() { continue; }
                        let lc = u.to_lowercase();
                        if lc.ends_with(".svg") || lc.contains("placeholder.svg") || lc.contains("iconify.design") { continue; }
                        if lc.contains("/assets/") { continue; }
                        if lc.ends_with(".jpg") || lc.ends_with(".jpeg") || lc.ends_with(".png") || lc.ends_with(".webp") || lc.ends_with(".avif") {
                            chosen = u.to_string();
                            break;
                        }
                    }
                    if !chosen.is_empty() { break; }
                }
                if chosen.is_empty() { continue; }
                let idx = count_from_img.unwrap_or_else(|| { let i = fallback_idx; fallback_idx += 1; i });
                ordered.push((idx, chosen));
            }
        }

        if !ordered.is_empty() {
            // Sort by index if any non-sequential values
            ordered.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, src) in ordered {
                if !seen_urls.iter().any(|u| u == &src) {
                    println!("loading image: {}", src);
                    seen_urls.push(src.clone());
                    pages.push(Page { content: PageContent::url(src), ..Default::default() });
                }
            }
        }
    }
    // Fallback: still limit to #pages but allow missing `count`, keep filters to avoid assets
    if pages.is_empty() {
        if let Some(pages_div) = html.select_first("#pages") {
            if let Some(mut imgs) = pages_div.select("img") {
                for img in &mut imgs {
                    let alt_lc = img.attr("alt").unwrap_or_default().to_lowercase();
                    if alt_lc.contains("donate") || alt_lc.contains("support") { continue; }
                    let mut cand: Vec<String> = Vec::new();
                    let s_src = img.attr("abs:src").unwrap_or_default(); if !s_src.is_empty() { cand.push(s_src); }
                    let s1 = img.attr("data-src").unwrap_or_default(); if !s1.is_empty() { cand.push(s1); }
                    let s2 = img.attr("data-original").unwrap_or_default(); if !s2.is_empty() { cand.push(s2); }
                    let s3 = img.attr("data-lazy-src").unwrap_or_default(); if !s3.is_empty() { cand.push(s3); }
                    let s4 = img.attr("data-url").unwrap_or_default(); if !s4.is_empty() { cand.push(s4); }
                    let sset = img.attr("srcset").unwrap_or_default(); if !sset.is_empty() { cand.push(sset); }
                    let mut src = String::new();
                    for c in cand {
                        let lc = c.to_lowercase();
                        if lc.contains(",") && lc.contains(" ") {
                            for part in c.split(',') {
                                let u = part.trim().split_whitespace().next().unwrap_or("");
                                let ulc = u.to_lowercase();
                                if ulc.is_empty() { continue; }
                                if ulc.ends_with(".svg") || ulc.contains("placeholder") || ulc.contains("iconify.design") || ulc.contains("calendar-badge") || ulc.contains("senkoheart") { continue; }
                                if ulc.ends_with(".jpg") || ulc.ends_with(".jpeg") || ulc.ends_with(".png") || ulc.ends_with(".webp") || ulc.ends_with(".avif") || ulc.starts_with("http://") || ulc.starts_with("https://") || ulc.starts_with("/") || ulc.starts_with("//") { src = u.to_string(); break; }
                            }
                            if !src.is_empty() { break; }
                            continue;
                        }
                        if lc.ends_with(".svg") || lc.contains("placeholder") || lc.contains("iconify.design") || lc.contains("calendar-badge") || lc.contains("senkoheart") { continue; }
                        if lc.ends_with(".jpg") || lc.ends_with(".jpeg") || lc.ends_with(".png") || lc.ends_with(".webp") || lc.ends_with(".avif") || lc.starts_with("http://") || lc.starts_with("https://") || lc.starts_with("/") || lc.starts_with("//") { src = c; break; }
                    }
                    if src.is_empty() { continue; }
                    if let Some(norm) = normalize_cover_url(&src) { src = norm; }
                    println!("loading image: {}", src);
                    pages.push(Page { content: PageContent::url(src), ..Default::default() });
                }
            }
            // Background-image style fallback inside #pages
            if pages.is_empty() {
                if let Some(mut styled) = pages_div.select("[style*=background-image], [style*='--photoURL:url(']") {
                    for el in &mut styled {
                        let st = el.attr("style").unwrap_or_default();
                        if st.is_empty() { continue; }
                        let s_lower = st.as_str();
                        let (start, end) = (s_lower.find("url("), s_lower.rfind(")"));
                        if let (Some(si), Some(ei)) = (start, end) {
                            if ei > si + 4 {
                                let mut v = String::from(&s_lower[si + 4..ei]);
                                if v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2 { v = v[1..v.len()-1].into(); }
                                if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 { v = v[1..v.len()-1].into(); }
                                let lc = v.to_lowercase();
                                if lc.ends_with(".svg") || lc.contains("placeholder") || lc.contains("iconify.design") || lc.contains("calendar-badge") || lc.contains("senkoheart") { continue; }
                                if let Some(norm) = normalize_cover_url(&v) { v = norm; }
                                println!("loading image: {}", v);
                                pages.push(Page { content: PageContent::url(v), ..Default::default() });
                            }
                        }
                    }
                }
            }
            // noscript fallback inside pages container
            if pages.is_empty() {
                if let Some(mut ns_imgs) = pages_div.select("noscript img") {
                    for img in &mut ns_imgs {
                        let mut src = img.attr("abs:src").unwrap_or_default();
                        if src.is_empty() { src = img.attr("src").unwrap_or_default(); }
                        if src.is_empty() { continue; }
                        let lc = src.to_lowercase();
                        if lc.ends_with(".svg") || lc.contains("placeholder") || lc.contains("iconify.design") || lc.contains("calendar-badge") || lc.contains("senkoheart") { continue; }
                        if let Some(norm) = normalize_cover_url(&src) { src = norm; }
                        if !seen_urls.iter().any(|u| u == &src) {
                            println!("loading image: {}", src);
                            seen_urls.push(src.clone());
                            pages.push(Page { content: PageContent::url(src), ..Default::default() });
                        }
                    }
                }
                if pages.is_empty() {
                    if let Some(mut ns_sources) = pages_div.select("noscript source[srcset]") {
                        for s in &mut ns_sources {
                            let entry = s.attr("srcset").unwrap_or_default();
                            if entry.is_empty() { continue; }
                            let mut picked = String::new();
                            for part in entry.split(',') {
                                let u = part.trim().split_whitespace().next().unwrap_or("");
                                let ulc = u.to_lowercase();
                                if ulc.is_empty() { continue; }
                                if ulc.ends_with(".svg") || ulc.contains("placeholder") || ulc.contains("iconify.design") || ulc.contains("calendar-badge") || ulc.contains("senkoheart") { continue; }
                                if ulc.starts_with("http://") || ulc.starts_with("https://") || ulc.starts_with("/") || ulc.starts_with("//") { picked = u.to_string(); break; }
                            }
                            if picked.is_empty() { continue; }
                            if let Some(norm) = normalize_cover_url(&picked) { picked = norm; }
                            if !seen_urls.iter().any(|u| u == &picked) {
                                println!("loading image: {}", picked);
                                seen_urls.push(picked.clone());
                                pages.push(Page { content: PageContent::url(picked), ..Default::default() });
                            }
                        }
                    }
                }
            }
        }
    }

    if pages.is_empty() {
        println!("no images found for chapter: {}", url);
        let text = match Request::new(url.clone(), HttpMethod::Get) {
            Ok(req) => match req.string() {
                Ok(s) => s,
                Err(_e) => { println!("failed to load page: {}", url); String::new() }
            },
            Err(_e) => { println!("failed to load page: {}", url); String::new() }
        };
        let mut slice = text.as_str();
        loop {
            // Look for any http(s) occurrence
            let pos_https = slice.find("https://");
            let pos_http = slice.find("http://");
            let pos = match (pos_https, pos_http) { (Some(a), Some(b)) => Some(a.min(b)), (Some(a), None) => Some(a), (None, Some(b)) => Some(b), (None, None) => None };
            if let Some(p) = pos {
                // Move to the beginning of the URL
                slice = &slice[p..];
                // End at a common delimiter: quote, whitespace, or angle bracket
                let mut end = slice.find('"').unwrap_or(slice.len());
                let ws = slice.find(' ');
                if let Some(w) = ws { if w < end { end = w; } }
                let gt = slice.find('>');
                if let Some(g) = gt { if g < end { end = g; } }
                let mut candidate = String::from(&slice[..end]);
                // Advance safely
                let adv = if end + 1 <= slice.len() { end + 1 } else { end };
                slice = &slice[adv..];
                let lc = candidate.to_lowercase();
                if !lc.contains("/assets/") && !lc.ends_with(".svg") && !lc.contains("placeholder") && !lc.contains("iconify.design") && !lc.contains("calendar-badge") && !lc.contains("senkoheart") {
                    // Normalize relative/protocol-relative as needed
                    if let Some(norm) = normalize_cover_url(&candidate) { candidate = norm; }
                    println!("loading image: {}", candidate);
                    pages.push(Page { content: PageContent::url(candidate), ..Default::default() });
                }
            } else { break; }
        }
    }

    Ok(pages)
}
