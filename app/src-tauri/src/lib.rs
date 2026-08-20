use bdo_rs::BDO;
use serde::{Deserialize, Serialize};
use sessionless::hex::IntoHex;
use sessionless::secp256k1::SecretKey;
use sessionless::Sessionless;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::Manager;

const MAX_CARDS: usize = 4;
const GATEWAY_BDO_URL: &str = "https://allyabase-gateway.netlify.app/bdo/";
const SAVAGE_URL: &str = "https://allyabase-gateway.netlify.app/savage/";
const BDO_HASH: &str = "bizbuz-card";

// ── Data types ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Social {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codeberg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linkedin: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default)]
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default)]
    pub social: Social,
    /// Base64-encoded JPEG (no `data:` prefix), resized/cropped client-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    /// BDO identity uuid this card is published under, once published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bdo_uuid: Option<String>,
    /// Full pre-signed savage URL — hosted by allyabase's own infrastructure
    /// (not a bizbuz-run server), rendering the SVG this card was published
    /// with as a live webpage. Computed locally at publish time (signing is
    /// local, no round-trip) and doesn't expire, so it's safe to treat as a
    /// permanent link rather than something that needs refreshing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CardsStore {
    cards: Vec<Profile>,
}

// ── Storage ──────────────────────────────────────────────────────────────────

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn cards_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("cards.json"))
}

/// Pre-multi-card storage location — a single `Profile` written directly as
/// JSON (no wrapping `cards` array). Migrated into cards.json on first load
/// if a user already had one card from before this feature existed.
fn legacy_profile_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("profile.json"))
}

fn write_cards(app: &tauri::AppHandle, store: &CardsStore) -> Result<(), String> {
    let path = cards_path(app)?;
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn read_cards(app: &tauri::AppHandle) -> Result<CardsStore, String> {
    let path = cards_path(app)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|e| e.to_string()),
        Err(_) => {
            // Nothing at cards.json yet — check for a pre-multi-card profile.json
            // and migrate it in as this user's first card.
            match fs::read_to_string(legacy_profile_path(app)?) {
                Ok(contents) => {
                    let mut profile: Profile =
                        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
                    if profile.id.is_empty() {
                        profile.id = new_id();
                    }
                    let store = CardsStore { cards: vec![profile] };
                    write_cards(app, &store)?;
                    Ok(store)
                }
                Err(_) => Ok(CardsStore::default()),
            }
        }
    }
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Nanosecond timestamp + a monotonic in-process counter, so two cards
/// created back-to-back never collide even if the clock's actual resolution
/// is coarser than a nanosecond (a plain millisecond timestamp collided
/// under exactly this scenario during testing).
fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}", nanos, counter)
}

fn unix_now_ms_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default()
}

// ── BDO publishing ───────────────────────────────────────────────────────────
//
// BDO's public storage is keyed by pubKey — one identity, one publicly
// retrievable slot, overwritten on every public write regardless of the
// `hash` used (verified directly against allyabase's db.js). So each card
// that gets published needs its own sessionless keypair, not one shared
// device identity. Keys live in their own file, separate from cards.json.

#[derive(Debug, Serialize, Deserialize, Clone)]
struct BdoKeypair {
    private_key_hex: String,
    pub_key_hex: String,
}

fn bdo_keys_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("bdo_keys.json"))
}

fn read_bdo_keys(app: &tauri::AppHandle) -> HashMap<String, BdoKeypair> {
    let path = match bdo_keys_path(app) {
        Ok(p) => p,
        Err(_) => return HashMap::new(),
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_bdo_keys(app: &tauri::AppHandle, keys: &HashMap<String, BdoKeypair>) -> Result<(), String> {
    let path = bdo_keys_path(app)?;
    let json = serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn sessionless_from_hex(priv_key_hex: &str) -> Result<Sessionless, String> {
    let bytes = hex::decode(priv_key_hex).map_err(|e| e.to_string())?;
    let secret_key = SecretKey::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Sessionless::from_private_key(secret_key))
}

/// Returns this card's BDO identity, generating and persisting a fresh
/// keypair the first time a given card is published.
fn load_or_create_bdo_sessionless(app: &tauri::AppHandle, card_id: &str) -> Result<Sessionless, String> {
    let mut keys = read_bdo_keys(app);
    if let Some(existing) = keys.get(card_id) {
        return sessionless_from_hex(&existing.private_key_hex);
    }

    let session = Sessionless::new();
    keys.insert(
        card_id.to_string(),
        BdoKeypair {
            private_key_hex: session.private_key().to_hex(),
            pub_key_hex: session.public_key().to_hex(),
        },
    );
    write_bdo_keys(app, &keys)?;
    Ok(session)
}

// ── SVG card rendering ───────────────────────────────────────────────────────
//
// savage only renders whatever `svg` property is already present on a
// published BDO's data — there's no server-side rendering step, so the app
// builds the visual card itself and publishes it alongside the structured
// fields.

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn get_initials(name: &str) -> String {
    let initials: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    }
}

/// Greedy word-wrap at `max_chars` per line, capped at `max_lines`. If the
/// text doesn't fit, the last line is trimmed and given a trailing ellipsis
/// rather than silently dropping the overflow.
fn wrap_text(text: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut i = 0;

    while i < words.len() && lines.len() < max_lines {
        let word = words[i];
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if candidate.chars().count() > max_chars && !current.is_empty() {
            lines.push(current.clone());
            current.clear();
        } else {
            current = candidate;
            i += 1;
        }
    }

    let truncated = i < words.len();
    if !current.is_empty() {
        lines.push(current);
    }

    if truncated {
        if let Some(last) = lines.last_mut() {
            while last.chars().count() + 1 > max_chars && !last.is_empty() {
                last.pop();
            }
            last.push('…');
        }
    }

    lines
}

/// Renders a `Profile` as a self-contained SVG business card. Mirrors the
/// dark/green/purple visual language already used in server.js's HTML card
/// and the app's own style.css.
fn render_card_svg(profile: &Profile) -> String {
    const WIDTH: u32 = 400;
    const BG: &str = "#0a001a";
    const GREEN: &str = "#10b981";
    const PURPLE: &str = "#a78bfa";

    let name = profile.name.clone().unwrap_or_else(|| "".to_string());
    let mut y: u32 = 190;
    let mut body = String::new();

    // Avatar
    let cx = WIDTH / 2;
    let cy: u32 = 110;
    let r: u32 = 60;
    if let Some(photo) = &profile.photo {
        body.push_str(&format!(
            r#"<defs><clipPath id="avatarClip"><circle cx="{cx}" cy="{cy}" r="{r}"/></clipPath></defs>
<image href="data:image/jpeg;base64,{photo}" x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="xMidYMid slice" clip-path="url(#avatarClip)"/>
<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{GREEN}" stroke-width="2"/>
"#,
            cx - r,
            cy - r,
            r * 2,
            r * 2,
        ));
    } else {
        body.push_str(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{GREEN}"/>
<text x="{cx}" y="{}" font-family="sans-serif" font-size="40" font-weight="bold" fill="{BG}" text-anchor="middle">{}</text>
"#,
            cy + 14,
            escape_xml(&get_initials(&name)),
        ));
    }

    // Name
    body.push_str(&format!(
        r#"<text x="{cx}" y="{y}" font-family="sans-serif" font-size="26" font-weight="bold" fill="{GREEN}" text-anchor="middle">{}</text>
"#,
        escape_xml(&name),
    ));

    if let Some(title) = profile.title.as_deref().filter(|s| !s.is_empty()) {
        y += 26;
        body.push_str(&format!(
            r#"<text x="{cx}" y="{y}" font-family="sans-serif" font-size="15" fill="{PURPLE}" text-anchor="middle">{}</text>
"#,
            escape_xml(title),
        ));
    }

    if let Some(company) = profile.company.as_deref().filter(|s| !s.is_empty()) {
        y += 22;
        body.push_str(&format!(
            r#"<text x="{cx}" y="{y}" font-family="sans-serif" font-size="13" fill="rgba(255,255,255,0.6)" text-anchor="middle">{}</text>
"#,
            escape_xml(company),
        ));
    }

    if let Some(bio) = profile.bio.as_deref().filter(|s| !s.is_empty()) {
        y += 34;
        let lines = wrap_text(bio, 42, 3);
        for line in &lines {
            body.push_str(&format!(
                r#"<text x="{cx}" y="{y}" font-family="sans-serif" font-style="italic" font-size="12" fill="rgba(255,255,255,0.7)" text-anchor="middle">"{}"</text>
"#,
                escape_xml(line),
            ));
            y += 17;
        }
        y -= 17;
    }

    y += 40;
    let contact_x: u32 = 40;
    let contact_row = |body: &mut String, y: &mut u32, icon: &str, label: String, href: Option<String>| {
        let escaped_label = escape_xml(&label);
        if let Some(href) = href {
            body.push_str(&format!(
                r#"<a href="{}"><text x="{contact_x}" y="{y}" font-family="sans-serif" font-size="14" fill="rgba(255,255,255,0.85)">{icon}  {escaped_label}</text></a>
"#,
                escape_xml(&href),
            ));
        } else {
            body.push_str(&format!(
                r#"<text x="{contact_x}" y="{y}" font-family="sans-serif" font-size="14" fill="rgba(255,255,255,0.85)">{icon}  {escaped_label}</text>
"#,
            ));
        }
        *y += 28;
    };

    if let Some(email) = profile.email.as_deref().filter(|s| !s.is_empty()) {
        contact_row(&mut body, &mut y, "@", email.to_string(), Some(format!("mailto:{email}")));
    }
    if let Some(phone) = profile.phone.as_deref().filter(|s| !s.is_empty()) {
        contact_row(&mut body, &mut y, "#", phone.to_string(), Some(format!("tel:{phone}")));
    }
    if let Some(website) = profile.website.as_deref().filter(|s| !s.is_empty()) {
        let href = if website.starts_with("http://") || website.starts_with("https://") {
            website.to_string()
        } else {
            format!("https://{website}")
        };
        contact_row(&mut body, &mut y, "~", website.to_string(), Some(href));
    }
    if let Some(location) = profile.location.as_deref().filter(|s| !s.is_empty()) {
        contact_row(&mut body, &mut y, "*", location.to_string(), None);
    }

    y += 24;
    body.push_str(&format!(
        r#"<text x="{cx}" y="{y}" font-family="sans-serif" font-size="11" fill="rgba(255,255,255,0.4)" text-anchor="middle">a Freyja offering</text>
"#
    ));

    let height = y + 24;

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}"><rect x="0" y="0" width="{WIDTH}" height="{height}" fill="{BG}"/>{body}</svg>"#
    )
}

// ── vCard rendering ──────────────────────────────────────────────────────────
//
// Mirrors shared/vcard.js's format exactly (field mapping, no escaping of
// vCard-special characters) so a card looks the same whether it was
// downloaded via the app's native share sheet or savage's "Save Contact"
// button on the public share page. JS can't run in savage's Node process on
// this card's data (it only has whatever's in the published BDO payload), so
// the vCard has to be pre-rendered here at publish time, same as the SVG.

/// Fold a vCard content line per RFC 2426 §2.6: no line may exceed 75
/// characters, and continuation lines start with a single leading space.
fn fold_vcard_line(line: &str) -> String {
    const LIMIT: usize = 75;
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= LIMIT {
        return line.to_string();
    }

    let mut result: String = chars[..LIMIT].iter().collect();
    let mut rest = &chars[LIMIT..];
    while !rest.is_empty() {
        let take = rest.len().min(LIMIT - 1);
        result.push_str("\r\n ");
        result.push_str(&rest[..take].iter().collect::<String>());
        rest = &rest[take..];
    }
    result
}

fn render_vcard(profile: &Profile) -> String {
    let mut lines: Vec<String> = vec!["BEGIN:VCARD".to_string(), "VERSION:3.0".to_string()];

    if let Some(name) = profile.name.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("FN:{name}"));
        let parts: Vec<&str> = name.split(' ').collect();
        if parts.len() >= 2 {
            lines.push(format!("N:{};{};;;", parts[1..].join(" "), parts[0]));
        } else {
            lines.push(format!("N:;{name};;;"));
        }
    }
    if let Some(title) = profile.title.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("TITLE:{title}"));
    }
    if let Some(company) = profile.company.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("ORG:{company}"));
    }
    if let Some(email) = profile.email.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("EMAIL;TYPE=INTERNET:{email}"));
    }
    if let Some(phone) = profile.phone.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("TEL;TYPE=CELL:{phone}"));
    }
    if let Some(website) = profile.website.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("URL:{website}"));
    }
    if let Some(location) = profile.location.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("ADR;TYPE=WORK:;;{location};;;;"));
    }
    if let Some(bio) = profile.bio.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("NOTE:{bio}"));
    }
    if let Some(github) = profile.social.github.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("URL;TYPE=GitHub:https://github.com/{github}"));
    }
    if let Some(codeberg) = profile.social.codeberg.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("URL;TYPE=Codeberg:https://codeberg.org/{codeberg}"));
    }
    if let Some(linkedin) = profile.social.linkedin.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("URL;TYPE=LinkedIn:https://linkedin.com/in/{linkedin}"));
    }
    if let Some(photo) = profile.photo.as_deref().filter(|s| !s.is_empty()) {
        lines.push(fold_vcard_line(&format!("PHOTO;ENCODING=b;TYPE=JPEG:{photo}")));
    }

    lines.push("END:VCARD".to_string());
    lines.join("\r\n")
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
async fn load_cards(app: tauri::AppHandle) -> Result<Vec<Profile>, String> {
    Ok(read_cards(&app)?.cards)
}

/// Upserts a card by id. Assigns a fresh id if the profile doesn't have one
/// yet (a new card). Rejects new cards once MAX_CARDS is reached.
#[tauri::command]
async fn save_card(app: tauri::AppHandle, mut profile: Profile) -> Result<Profile, String> {
    let mut store = read_cards(&app)?;
    let is_new = profile.id.is_empty() || !store.cards.iter().any(|c| c.id == profile.id);

    if is_new {
        if store.cards.len() >= MAX_CARDS {
            return Err(format!("You can only keep up to {} cards.", MAX_CARDS));
        }
        if profile.id.is_empty() {
            profile.id = new_id();
        }
    }

    store.cards.retain(|c| c.id != profile.id);
    store.cards.push(profile.clone());
    write_cards(&app, &store)?;
    Ok(profile)
}

#[tauri::command]
async fn delete_card(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut store = read_cards(&app)?;
    store.cards.retain(|c| c.id != id);
    write_cards(&app, &store)
}

/// Publishes (or re-publishes, pushing edits) a card to BDO, embedding a
/// rendered SVG so savage can serve it as a live shareable webpage. The link
/// is available the instant this returns — the savage URL is a locally
/// computed signature, not a server-assigned code to wait on.
#[tauri::command]
async fn publish_card(app: tauri::AppHandle, card_id: String) -> Result<Profile, String> {
    let mut store = read_cards(&app)?;
    let index = store
        .cards
        .iter()
        .position(|c| c.id == card_id)
        .ok_or_else(|| "Card not found".to_string())?;

    let sessionless = load_or_create_bdo_sessionless(&app, &card_id)?;
    let client = BDO::new(Some(GATEWAY_BDO_URL.to_string()), Some(sessionless));

    let svg = render_card_svg(&store.cards[index]);
    let vcard = render_vcard(&store.cards[index]);
    let mut card_json = serde_json::to_value(&store.cards[index]).map_err(|e| e.to_string())?;
    let card_obj = card_json
        .as_object_mut()
        .ok_or_else(|| "card serialized to non-object".to_string())?;
    card_obj.insert("svg".to_string(), serde_json::Value::String(svg));
    card_obj.insert("vcard".to_string(), serde_json::Value::String(vcard));

    let existing_uuid = store.cards[index].bdo_uuid.clone();

    let uuid = if let Some(uuid) = existing_uuid {
        client
            .update_bdo(&uuid, BDO_HASH, &card_json, &true)
            .await
            .map_err(|e| e.to_string())?;
        uuid
    } else {
        let user = client
            .create_user(BDO_HASH, &card_json, &true)
            .await
            .map_err(|e| e.to_string())?;
        user.uuid
    };

    let timestamp = unix_now_ms_string();
    let signature = client
        .sessionless
        .sign(format!("{timestamp}{uuid}{BDO_HASH}"))
        .to_hex();
    let share_url = format!(
        "{SAVAGE_URL}user/{uuid}/bdo?timestamp={timestamp}&hash={BDO_HASH}&signature={signature}"
    );

    let card = &mut store.cards[index];
    card.bdo_uuid = Some(uuid);
    card.share_url = Some(share_url);
    card.published_at = Some(unix_now_ms_string());
    let updated = card.clone();

    write_cards(&app, &store)?;
    Ok(updated)
}

// ── Referral link ────────────────────────────────────────────────────────────
//
// Hosted exactly like a card: an SVG published on its own BDO, wrapped as a
// live webpage by savage. One referral identity per app install (not per
// card — this is about referring new people to the app itself, not tied to
// any specific business card), reusing the same per-key BDO identity
// mechanism as cards (`load_or_create_bdo_sessionless`) under the fixed key
// "referral" rather than a card id.
//
// savage's sanitizer strips <script>/javascript:/data: content from the svg
// (see render_card_svg's doc comment), so an automatic redirect to the App
// Store can't survive being embedded in the svg - instead the svg just has
// a plain "Get BizBuz" button, a real <a href> to the App Store, which is
// exactly the same kind of link the card's own contact rows already use.

const REFERRAL_HASH: &str = "bizbuz-referral";
// TODO: replace with the real App Store listing URL once BizBuz has one.
const APP_STORE_URL: &str = "https://apps.apple.com/app/id0000000000";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ReferralLink {
    uuid: String,
    share_url: String,
}

fn referral_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("referral.json"))
}

fn read_referral_link(app: &tauri::AppHandle) -> Option<ReferralLink> {
    let path = referral_path(app).ok()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_referral_link(app: &tauri::AppHandle, link: &ReferralLink) -> Result<(), String> {
    let path = referral_path(app)?;
    let json = serde_json::to_string_pretty(link).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Renders the referral SVG: a simplified echo of the app icon's
/// stacked-cards mark, a tagline, and a "Get BizBuz" button linking to the
/// App Store.
fn render_referral_svg(app_store_url: &str) -> String {
    const WIDTH: u32 = 400;
    const HEIGHT: u32 = 440;
    const BG: &str = "#0a001a";
    const GREEN: &str = "#10b981";
    const PURPLE: &str = "#a78bfa";
    const CARD_BACK: &str = "#4c3d73";

    let cx = WIDTH / 2;
    let button_y: u32 = 300;

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<defs><linearGradient id="markGradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="{GREEN}"/><stop offset="100%" stop-color="{PURPLE}"/></linearGradient></defs>
<rect x="0" y="0" width="{WIDTH}" height="{HEIGHT}" fill="{BG}"/>
<rect x="{}" y="70" width="140" height="95" rx="18" fill="{CARD_BACK}"/>
<rect x="{}" y="95" width="140" height="95" rx="18" fill="url(#markGradient)"/>
<circle cx="{}" cy="122" r="11" fill="{BG}"/>
<text x="{cx}" y="210" font-family="sans-serif" font-size="34" font-weight="bold" fill="url(#markGradient)" text-anchor="middle">BizBuz</text>
<text x="{cx}" y="240" font-family="sans-serif" font-size="14" fill="rgba(255,255,255,0.7)" text-anchor="middle">Digital business cards, made simple.</text>
<text x="{cx}" y="270" font-family="sans-serif" font-size="14" fill="{PURPLE}" text-anchor="middle">You've been invited to try it out.</text>
<a href="{}"><rect x="{}" y="{button_y}" width="240" height="56" rx="16" fill="{GREEN}"/><text x="{cx}" y="{}" font-family="sans-serif" font-size="18" font-weight="bold" fill="{BG}" text-anchor="middle">Get BizBuz</text></a>
<text x="{cx}" y="400" font-family="sans-serif" font-size="11" fill="rgba(255,255,255,0.4)" text-anchor="middle">a Freyja offering</text>
</svg>"#,
        cx - 90,
        cx - 70,
        cx - 55,
        escape_xml(app_store_url),
        cx - 120,
        button_y + 36,
    )
}

/// Returns this install's referral share URL — publishing an svg to its own
/// fresh BDO and computing the permanent savage URL the first time this is
/// called (mirrors `publish_card` exactly), and reusing the result (from
/// local storage) on every call after that, since the referral svg is
/// static and never needs re-publishing.
#[tauri::command]
async fn get_or_create_referral_link(app: tauri::AppHandle) -> Result<String, String> {
    if let Some(link) = read_referral_link(&app) {
        return Ok(link.share_url);
    }

    let sessionless = load_or_create_bdo_sessionless(&app, "referral")?;
    let client = BDO::new(Some(GATEWAY_BDO_URL.to_string()), Some(sessionless));

    let svg = render_referral_svg(APP_STORE_URL);
    let card_json = serde_json::json!({ "svg": svg });

    let user = client
        .create_user(REFERRAL_HASH, &card_json, &true)
        .await
        .map_err(|e| e.to_string())?;
    let uuid = user.uuid;

    let timestamp = unix_now_ms_string();
    let signature = client
        .sessionless
        .sign(format!("{timestamp}{uuid}{REFERRAL_HASH}"))
        .to_hex();
    let share_url = format!(
        "{SAVAGE_URL}user/{uuid}/bdo?timestamp={timestamp}&hash={REFERRAL_HASH}&signature={signature}"
    );

    let link = ReferralLink { uuid, share_url: share_url.clone() };
    write_referral_link(&app, &link)?;
    Ok(share_url)
}

// ── App entry ────────────────────────────────────────────────────────────────
//
// NOTE: tauri-plugin-quick-actions is temporarily NOT registered — its
// source (Rust + Swift) was lost to an accidental `git clean` and hasn't
// been rebuilt yet. ios-native/BizbuzQuickActionsBridge.m (the native
// swizzling bridge that writes a pending shortcut item to UserDefaults) is
// still here and harmless to compile in, but nothing currently reads that
// value back out to JS. See the frontend for the matching removal of
// syncQuickActions/checkPendingQuickAction.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_share_sheet::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            load_cards,
            save_card,
            delete_card,
            publish_card,
            get_or_create_referral_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running bizbuz");
}
