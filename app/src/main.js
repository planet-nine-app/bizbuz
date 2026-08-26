import { generateVCard, vcardFilename } from './lib/vcard.js';

const { core, dialog, fs } = window.__TAURI__;

const MAX_CARDS = 4;
// Cap on the canonical profile's field list — keep in sync with
// MAX_PROFILE_FIELDS in src-tauri/src/lib.rs.
const MAX_PROFILE_FIELDS = 20;

const cardsView = document.getElementById('cards-view');
const editView = document.getElementById('edit-view');
const cardView = document.getElementById('card-view');
const cardsGrid = document.getElementById('cards-grid');
const form = document.getElementById('profile-form');
const statusMsg = document.getElementById('status-msg');
const photoPreview = document.getElementById('photo-preview');
const chooseePhotoBtn = document.getElementById('choose-photo-btn');
const cancelEditBtn = document.getElementById('cancel-edit-btn');
const deleteCardBtn = document.getElementById('delete-card-btn');
const publishBtn = document.getElementById('publish-btn');
const linkRow = document.getElementById('link-row');
const linkText = document.getElementById('link-text');
const copyLinkBtn = document.getElementById('copy-link-btn');
const referralShareBtn = document.getElementById('referral-share-btn');
const importLinkitylinkBtn = document.getElementById('import-linkitylink-btn');
const shareAppGroupBtn = document.getElementById('share-app-group-btn');
const profileNavBtn = document.getElementById('profile-nav-btn');
const profileView = document.getElementById('profile-view');
const profileForm = document.getElementById('canonical-profile-form');
const profilePhotoPreview = document.getElementById('profile-photo-preview');
const profileChoosePhotoBtn = document.getElementById('profile-choose-photo-btn');
const profilePullBtn = document.getElementById('profile-pull-btn');
const profileFieldsEl = document.getElementById('profile-fields');
const profileNewFieldName = document.getElementById('profile-new-field-name');
const profileNewFieldValue = document.getElementById('profile-new-field-value');
const profileAddFieldBtn = document.getElementById('profile-add-field-btn');
const profileFieldLimitHint = document.getElementById('profile-field-limit-hint');
const profileCloseBtn = document.getElementById('profile-close-btn');

const PHOTO_SIZE = 480;
const PHOTO_QUALITY = 0.85;

let cards = [];
let activeCard = null; // the card shown in card-view / being edited in edit-view (null = creating new)
let pendingPhoto = null; // base64 JPEG (no data: prefix), staged from the picker until Save
let deleteArmed = false;
let deleteArmedTimeout = null;

// ── View / status helpers ────────────────────────────────────────────────────

function showView(name) {
    cardsView.hidden = name !== 'cards';
    editView.hidden = name !== 'edit';
    cardView.hidden = name !== 'card';
    profileView.hidden = name !== 'profile';
    profileNavBtn.hidden = name === 'profile';
}

let statusTimeout = null;
function setStatus(message) {
    statusMsg.textContent = message;
    statusMsg.classList.add('visible');
    clearTimeout(statusTimeout);
    statusTimeout = setTimeout(() => statusMsg.classList.remove('visible'), 2500);
}

function getInitials(name) {
    if (!name) return '?';
    return name
        .split(/\s+/)
        .filter(Boolean)
        .map((word) => word[0])
        .join('')
        .slice(0, 2)
        .toUpperCase();
}

function setAvatarContent(el, photo, name) {
    if (photo) {
        el.style.backgroundImage = `url(data:image/jpeg;base64,${photo})`;
        el.textContent = '';
    } else {
        el.style.backgroundImage = '';
        el.textContent = getInitials(name);
    }
}

// ── Photo picker ──────────────────────────────────────────────────────────────

async function resizeImageToJpegBase64(bytes) {
    const blob = new Blob([bytes]);
    const bitmap = await createImageBitmap(blob);

    const scale = Math.min(1, PHOTO_SIZE / Math.max(bitmap.width, bitmap.height));
    const width = Math.round(bitmap.width * scale);
    const height = Math.round(bitmap.height * scale);

    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(bitmap, 0, 0, width, height);

    const dataUrl = canvas.toDataURL('image/jpeg', PHOTO_QUALITY);
    return dataUrl.split(',')[1];
}

chooseePhotoBtn.addEventListener('click', async () => {
    try {
        const path = await dialog.open({
            multiple: false,
            filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'heic'] }],
        });
        if (!path) return;

        const bytes = await fs.readFile(path);
        pendingPhoto = await resizeImageToJpegBase64(bytes);
        setAvatarContent(photoPreview, pendingPhoto, '');
    } catch (err) {
        setStatus(`Couldn't set photo: ${err}`);
    }
});

// ── App Group sharing ────────────────────────────────────────────────────────
//
// Import fills only currently-empty fields — additive, not destructive,
// consistent with an in-progress edit taking priority over an import.

importLinkitylinkBtn.addEventListener('click', async () => {
    importLinkitylinkBtn.disabled = true;
    setStatus('Checking for a shared Linkitylink card…');
    try {
        const r = await core.invoke('import_from_linkitylink');
        const nameEl = document.getElementById('field-name');
        const bioEl = document.getElementById('field-bio');
        const websiteEl = document.getElementById('field-website');
        const githubEl = document.getElementById('field-github');
        const codebergEl = document.getElementById('field-codeberg');
        const linkedinEl = document.getElementById('field-linkedin');
        if (r.name && !nameEl.value.trim()) nameEl.value = r.name;
        if (r.bio && !bioEl.value.trim()) bioEl.value = r.bio;
        if (r.photo && !pendingPhoto) {
            pendingPhoto = r.photo;
            setAvatarContent(photoPreview, pendingPhoto, nameEl.value || '');
        }
        if (r.website && !websiteEl.value.trim()) websiteEl.value = r.website;
        if (r.github && !githubEl.value.trim()) githubEl.value = r.github;
        if (r.codeberg && !codebergEl.value.trim()) codebergEl.value = r.codeberg;
        if (r.linkedin && !linkedinEl.value.trim()) linkedinEl.value = r.linkedin;
        setStatus(r.skippedCount > 0
            ? `Imported from Linkitylink — ${r.skippedCount} link${r.skippedCount === 1 ? '' : 's'} couldn't be imported (BizBuz only supports GitHub, Codeberg, LinkedIn, and one website link).`
            : 'Imported from Linkitylink!');
    } catch (err) {
        setStatus(`${err}`); // Rust already returns a complete, user-facing sentence
    } finally {
        importLinkitylinkBtn.disabled = false;
    }
});

shareAppGroupBtn.addEventListener('click', async () => {
    if (!activeCard) return;
    shareAppGroupBtn.disabled = true;
    try {
        await core.invoke('share_card_to_app_group', { cardId: activeCard.id });
        setStatus('Shared — Linkitylink can now import this profile.');
    } catch (err) {
        setStatus(`Couldn't share: ${err}`);
    } finally {
        shareAppGroupBtn.disabled = false;
    }
});

// ── Form <-> Profile ──────────────────────────────────────────────────────────

function profileFromForm() {
    return {
        name: document.getElementById('field-name').value.trim(),
        title: document.getElementById('field-title').value.trim() || undefined,
        company: document.getElementById('field-company').value.trim() || undefined,
        email: document.getElementById('field-email').value.trim() || undefined,
        phone: document.getElementById('field-phone').value.trim() || undefined,
        website: document.getElementById('field-website').value.trim() || undefined,
        location: document.getElementById('field-location').value.trim() || undefined,
        bio: document.getElementById('field-bio').value.trim() || undefined,
        social: {
            github: document.getElementById('field-github').value.trim() || undefined,
            codeberg: document.getElementById('field-codeberg').value.trim() || undefined,
            linkedin: document.getElementById('field-linkedin').value.trim() || undefined,
        },
        photo: pendingPhoto || undefined,
    };
}

function fillForm(card) {
    document.getElementById('field-name').value = card.name || '';
    document.getElementById('field-title').value = card.title || '';
    document.getElementById('field-company').value = card.company || '';
    document.getElementById('field-email').value = card.email || '';
    document.getElementById('field-phone').value = card.phone || '';
    document.getElementById('field-website').value = card.website || '';
    document.getElementById('field-location').value = card.location || '';
    document.getElementById('field-bio').value = card.bio || '';
    document.getElementById('field-github').value = card.social?.github || '';
    document.getElementById('field-codeberg').value = card.social?.codeberg || '';
    document.getElementById('field-linkedin').value = card.social?.linkedin || '';
    pendingPhoto = card.photo || null;
    setAvatarContent(photoPreview, card.photo, card.name || '');
}

// ── Cards grid ────────────────────────────────────────────────────────────────

function renderCardsGrid() {
    cardsGrid.innerHTML = '';

    for (const card of cards) {
        const tile = document.createElement('button');
        tile.type = 'button';
        tile.className = 'card-tile filled';

        const avatar = document.createElement('div');
        avatar.className = 'avatar';
        setAvatarContent(avatar, card.photo, card.name || '');
        tile.appendChild(avatar);

        const name = document.createElement('span');
        name.className = 'card-tile-name';
        name.textContent = card.name || 'Untitled';
        tile.appendChild(name);

        tile.addEventListener('click', () => openCard(card));
        cardsGrid.appendChild(tile);
    }

    if (cards.length < MAX_CARDS) {
        const addTile = document.createElement('button');
        addTile.type = 'button';
        addTile.className = 'card-tile empty';
        addTile.textContent = '+';
        addTile.addEventListener('click', openNewCardForm);
        cardsGrid.appendChild(addTile);
    }
}

function renderPublishLink(card) {
    if (card.shareUrl) {
        linkText.textContent = card.shareUrl;
        linkRow.hidden = false;
        publishBtn.textContent = 'Update Shareable Link';
    } else {
        linkRow.hidden = true;
        publishBtn.textContent = 'Get Shareable Link';
    }
}

// Upserts a card into the in-memory `cards` list and, if it's the one
// currently on screen, refreshes the link row too — used by both the
// explicit Publish button and the silent background sync below.
function applyCardUpdate(updated) {
    cards = cards.filter((c) => c.id !== updated.id);
    cards.push(updated);
    if (activeCard?.id === updated.id) {
        activeCard = updated;
        renderPublishLink(updated);
    }
}

// The shareable link (shareUrl, a pre-signed savage URL) is available the
// instant publish_card returns — the signature never expires, so it's
// computed once and doesn't need to be refreshed on later shares. Publishing
// proactively in the background (on boot and after every edit, not only when
// the user taps the button) keeps the published card in sync with the latest
// edits without requiring an explicit re-publish tap first.
async function backgroundPublishCard(cardId) {
    try {
        const updated = await core.invoke('publish_card', { cardId });
        applyCardUpdate(updated);
    } catch {
        // Best-effort — network hiccups etc. shouldn't surface to the user
        // for a sync they didn't explicitly ask for.
    }
}

function backgroundPublishAllCards() {
    for (const card of cards) backgroundPublishCard(card.id);
}

// ── Card view ─────────────────────────────────────────────────────────────────

function renderCard(card) {
    setAvatarContent(document.getElementById('card-avatar'), card.photo, card.name || '');
    document.getElementById('card-name').textContent = card.name || 'Untitled';

    const titleEl = document.getElementById('card-title');
    titleEl.textContent = card.title || '';
    titleEl.hidden = !card.title;

    const companyEl = document.getElementById('card-company');
    companyEl.textContent = card.company || '';
    companyEl.hidden = !card.company;

    const bioEl = document.getElementById('card-bio');
    bioEl.textContent = card.bio ? `"${card.bio}"` : '';
    bioEl.hidden = !card.bio;

    const contact = document.getElementById('card-contact');
    contact.innerHTML = '';

    const addRow = (icon, label, href) => {
        const el = document.createElement(href ? 'a' : 'div');
        el.className = 'contact-item';
        if (href) el.href = href;
        el.innerHTML = `<span class="contact-icon">${icon}</span><span></span>`;
        el.querySelector('span:last-child').textContent = label;
        contact.appendChild(el);
    };

    if (card.email) addRow('@', card.email, `mailto:${card.email}`);
    if (card.phone) addRow('#', card.phone, `tel:${card.phone}`);
    if (card.website) {
        const href = card.website.startsWith('http') ? card.website : `https://${card.website}`;
        addRow('~', card.website.replace(/^https?:\/\//, ''), href);
    }
    if (card.location) addRow('*', card.location, null);
}

function openCard(card) {
    activeCard = card;
    renderCard(card);
    renderPublishLink(card);
    showView('card');
}

function openNewCardForm() {
    activeCard = null;
    pendingPhoto = null;
    disarmDelete();
    form.reset();
    setAvatarContent(photoPreview, null, '');
    cancelEditBtn.hidden = cards.length === 0;
    deleteCardBtn.hidden = true;
    showView('edit');
}

async function loadCards() {
    cards = await core.invoke('load_cards');
    if (cards.length === 0) {
        openNewCardForm();
    } else {
        renderCardsGrid();
        showView('cards');
    }
}

form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const profile = profileFromForm();
    if (activeCard) profile.id = activeCard.id;

    try {
        const saved = await core.invoke('save_card', { profile });
        cards = cards.filter((c) => c.id !== saved.id);
        cards.push(saved);
        activeCard = saved;
        renderCardsGrid();
        renderCard(saved);
        renderPublishLink(saved);
        showView('card');
        backgroundPublishCard(saved.id);
    } catch (err) {
        setStatus(`Couldn't save: ${err}`);
    }
});

cancelEditBtn.addEventListener('click', () => {
    showView(activeCard ? 'card' : 'cards');
});

function disarmDelete() {
    deleteArmed = false;
    clearTimeout(deleteArmedTimeout);
    deleteCardBtn.textContent = 'Delete Card';
}

deleteCardBtn.addEventListener('click', async () => {
    if (!activeCard) return;

    if (!deleteArmed) {
        deleteArmed = true;
        deleteCardBtn.textContent = 'Tap again to confirm';
        deleteArmedTimeout = setTimeout(disarmDelete, 3000);
        return;
    }

    disarmDelete();
    try {
        await core.invoke('delete_card', { id: activeCard.id });
        cards = cards.filter((c) => c.id !== activeCard.id);
        activeCard = null;
        if (cards.length === 0) {
            openNewCardForm();
        } else {
            renderCardsGrid();
            showView('cards');
        }
    } catch (err) {
        setStatus(`Couldn't delete: ${err}`);
    }
});

document.getElementById('edit-btn').addEventListener('click', () => {
    if (activeCard) fillForm(activeCard);
    disarmDelete();
    cancelEditBtn.hidden = false;
    deleteCardBtn.hidden = false;
    showView('edit');
});

document.getElementById('back-to-cards-btn').addEventListener('click', () => {
    activeCard = null;
    renderCardsGrid();
    showView('cards');
});

async function shareCard(card) {
    const vcard = generateVCard(card);
    const fileName = vcardFilename(card);
    await core.invoke('plugin:share-sheet|share_file', {
        fileName,
        contents: vcard,
        mimeType: 'text/vcard',
    });
}

document.getElementById('share-btn').addEventListener('click', async () => {
    if (!activeCard) return;
    try {
        await shareCard(activeCard);
    } catch (err) {
        setStatus(`Couldn't share: ${err}`);
    }
});

publishBtn.addEventListener('click', async () => {
    if (!activeCard) return;
    publishBtn.disabled = true;
    setStatus('Publishing…');
    try {
        const updated = await core.invoke('publish_card', { cardId: activeCard.id });
        applyCardUpdate(updated);
        setStatus('Link ready!');
    } catch (err) {
        setStatus(`Couldn't publish: ${err}`);
    } finally {
        publishBtn.disabled = false;
    }
});

copyLinkBtn.addEventListener('click', async () => {
    try {
        await navigator.clipboard.writeText(linkText.textContent);
        setStatus('Copied!');
    } catch (err) {
        setStatus(`Couldn't copy: ${err}`);
    }
});

// ── Referral link ─────────────────────────────────────────────────────────────
//
// No preview/copy row — just a button. Tapping it asks Rust for this
// install's referral link (registering one with BDO/savage on first launch,
// reusing the cached one after that) and hands it straight to the native
// share sheet as a real URL.
referralShareBtn.addEventListener('click', async () => {
    referralShareBtn.disabled = true;
    try {
        const url = await core.invoke('get_or_create_referral_link');
        await core.invoke('plugin:share-sheet|share_text', { text: url });
    } catch (err) {
        setStatus(`Couldn't share: ${err}`);
    } finally {
        referralShareBtn.disabled = false;
    }
});

// ── Canonical profile ────────────────────────────────────────────────────────
//
// A separate, App-Group-shared record — independent of `cards`/`activeCard`
// above. Its own photo/field-list state is kept apart from the card editor's
// (pendingPhoto/etc. would be the wrong variables here) so editing this
// screen can never bleed into whichever card is currently being edited.
// Every field (built-in or pulled-in) is a {slug, name, value} tuple — the
// slug is the stable machine identity used for dedup, `name` is a human
// label, `value` is the content. Mirrors slugify() in src-tauri/src/lib.rs.

let preProfileView = 'cards'; // the view to return to on Close/Save
let pendingProfilePhoto = null;
let pendingProfileFields = [];
let editingProfileFieldIndex = null;

function slugify(s) {
    return s
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '_')
        .replace(/^_+|_+$/g, '');
}

function renderProfileFields() {
    profileFieldsEl.innerHTML = '';

    pendingProfileFields.forEach((entry, index) => {
        const li = document.createElement('li');
        li.className = 'link-entry';

        if (index === editingProfileFieldIndex) {
            const fields = document.createElement('div');
            fields.className = 'link-entry-edit-fields';

            const nameInput = document.createElement('input');
            nameInput.type = 'text';
            nameInput.placeholder = 'Field';
            nameInput.maxLength = 40;
            nameInput.value = entry.name;

            const valueInput = document.createElement('input');
            valueInput.type = 'text';
            valueInput.placeholder = 'Value';
            valueInput.value = entry.value;

            const commit = () => {
                entry.name = nameInput.value.trim();
                entry.value = valueInput.value.trim();
                entry.slug = slugify(entry.name);
                editingProfileFieldIndex = null;
                renderProfileFields();
            };
            const onEnter = (e) => { if (e.key === 'Enter') commit(); };
            nameInput.addEventListener('keydown', onEnter);
            valueInput.addEventListener('keydown', onEnter);

            fields.append(nameInput, valueInput);
            li.appendChild(fields);

            const doneBtn = document.createElement('button');
            doneBtn.type = 'button';
            doneBtn.textContent = '✓';
            doneBtn.addEventListener('click', commit);

            const actions = document.createElement('div');
            actions.className = 'link-entry-actions';
            actions.appendChild(doneBtn);
            li.appendChild(actions);

            profileFieldsEl.appendChild(li);
            nameInput.focus();
            return;
        }

        const text = document.createElement('div');
        text.className = 'link-entry-text';
        text.innerHTML = '<div class="link-entry-label"></div><div class="link-entry-url"></div>';
        text.querySelector('.link-entry-label').textContent = entry.name || entry.slug;
        text.querySelector('.link-entry-url').textContent = entry.value;
        text.addEventListener('click', () => {
            editingProfileFieldIndex = index;
            renderProfileFields();
        });
        li.appendChild(text);

        const actions = document.createElement('div');
        actions.className = 'link-entry-actions';

        const upBtn = document.createElement('button');
        upBtn.type = 'button';
        upBtn.textContent = '↑';
        upBtn.disabled = index === 0;
        upBtn.addEventListener('click', () => {
            [pendingProfileFields[index - 1], pendingProfileFields[index]] = [pendingProfileFields[index], pendingProfileFields[index - 1]];
            renderProfileFields();
        });

        const downBtn = document.createElement('button');
        downBtn.type = 'button';
        downBtn.textContent = '↓';
        downBtn.disabled = index === pendingProfileFields.length - 1;
        downBtn.addEventListener('click', () => {
            [pendingProfileFields[index], pendingProfileFields[index + 1]] = [pendingProfileFields[index + 1], pendingProfileFields[index]];
            renderProfileFields();
        });

        const removeBtn = document.createElement('button');
        removeBtn.type = 'button';
        removeBtn.textContent = '×';
        removeBtn.addEventListener('click', () => {
            pendingProfileFields.splice(index, 1);
            if (editingProfileFieldIndex === index) editingProfileFieldIndex = null;
            renderProfileFields();
        });

        actions.append(upBtn, downBtn, removeBtn);
        li.appendChild(actions);
        profileFieldsEl.appendChild(li);
    });

    const atLimit = pendingProfileFields.length >= MAX_PROFILE_FIELDS;
    profileAddFieldBtn.disabled = atLimit;
    profileFieldLimitHint.hidden = !atLimit;
}

profileAddFieldBtn.addEventListener('click', () => {
    const name = profileNewFieldName.value.trim();
    const value = profileNewFieldValue.value.trim();
    if (!name || !value || pendingProfileFields.length >= MAX_PROFILE_FIELDS) return;

    pendingProfileFields.push({ slug: slugify(name), name, value });
    profileNewFieldName.value = '';
    profileNewFieldValue.value = '';
    renderProfileFields();
});

profileChoosePhotoBtn.addEventListener('click', async () => {
    try {
        const path = await dialog.open({
            multiple: false,
            filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'heic'] }],
        });
        if (!path) return;

        const bytes = await fs.readFile(path);
        pendingProfilePhoto = await resizeImageToJpegBase64(bytes);
        setAvatarContent(profilePhotoPreview, pendingProfilePhoto, '');
    } catch (err) {
        setStatus(`Couldn't set photo: ${err}`);
    }
});

const BIZBUZ_FIELD_DEFS = [
    ['name', 'Name'], ['title', 'Title'], ['company', 'Company'], ['email', 'Email'],
    ['phone', 'Phone'], ['website', 'Website'], ['location', 'Location'], ['bio', 'Bio'],
];
const BIZBUZ_SOCIAL_DEFS = [['github', 'GitHub'], ['codeberg', 'Codeberg'], ['linkedin', 'LinkedIn']];

// Local-only: reads the `cards` array already loaded in memory (all of
// BizBuz's own saved cards, not just the active one). First-non-empty-per-
// slug wins; never overwrites a field already present in the profile.
profilePullBtn.addEventListener('click', () => {
    const existingSlugs = new Set(pendingProfileFields.map((f) => f.slug));
    for (const c of cards) {
        for (const [key, label] of BIZBUZ_FIELD_DEFS) {
            if (c[key] && !existingSlugs.has(key)) {
                pendingProfileFields.push({ slug: key, name: label, value: c[key] });
                existingSlugs.add(key);
            }
        }
        for (const [key, label] of BIZBUZ_SOCIAL_DEFS) {
            const value = c.social?.[key];
            if (value && !existingSlugs.has(key)) {
                pendingProfileFields.push({ slug: key, name: label, value });
                existingSlugs.add(key);
            }
        }
        if (c.photo && !pendingProfilePhoto) {
            pendingProfilePhoto = c.photo;
            setAvatarContent(profilePhotoPreview, pendingProfilePhoto, '');
        }
    }
    renderProfileFields();
    setStatus('Pulled in fields from your cards.');
});

function fillProfileForm(profile) {
    pendingProfilePhoto = profile?.photo || null;
    pendingProfileFields = (profile?.fields || []).map((f) => ({ ...f }));
    editingProfileFieldIndex = null;
    setAvatarContent(profilePhotoPreview, profile?.photo, '');
    renderProfileFields();
}

function canonicalProfileFromForm() {
    return {
        photo: pendingProfilePhoto || undefined,
        fields: pendingProfileFields,
    };
}

function currentViewName() {
    if (!cardsView.hidden) return 'cards';
    if (!editView.hidden) return 'edit';
    if (!cardView.hidden) return 'card';
    return 'cards';
}

profileNavBtn.addEventListener('click', async () => {
    preProfileView = currentViewName();
    try {
        const profile = await core.invoke('load_canonical_profile');
        fillProfileForm(profile);
        showView('profile');
    } catch (err) {
        setStatus(`Couldn't load profile: ${err}`);
    }
});

profileForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    try {
        await core.invoke('save_canonical_profile', { profile: canonicalProfileFromForm() });
        setStatus('Profile saved — shared across your apps.');
        showView(preProfileView);
    } catch (err) {
        setStatus(`Couldn't save: ${err}`);
    }
});

profileCloseBtn.addEventListener('click', () => showView(preProfileView));

loadCards().then(() => {
    backgroundPublishAllCards();
});
