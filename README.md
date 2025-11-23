# BizBuz

Digital business cards powered by Planet Nine profiles.

## Overview

BizBuz creates beautiful digital business cards from your Prof profile. Share your card via QR code, and let contacts save your info with one tap.

## Features

- **Digital Business Cards** - Beautiful, responsive cards with your professional info
- **QR Code Sharing** - Generate scannable QR codes for easy sharing
- **vCard Download** - One-tap save to phone contacts
- **Prof Integration** - Automatically pulls from your Planet Nine profile
- **No Tracking** - Privacy-first, no analytics or data collection

## Quick Start

```bash
# Install dependencies
npm install

# Start server
npm start

# Development mode with auto-reload
npm run dev
```

Server runs on `http://localhost:3011` by default.

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Landing page |
| `GET /card/:identifier` | View business card (uuid, pubKey, or 'demo') |
| `GET /vcard/:identifier` | Download vCard file for contact saving |
| `GET /qr/:identifier` | Generate QR code image for card URL |

## Demo

Visit `/card/demo` to see a sample business card.

## Environment Variables

```bash
PORT=3011                    # Server port (default: 3011)
PROF_BASE_URL=http://localhost:3012  # Prof service URL
```

## How It Works

1. User creates a Prof profile with contact information
2. BizBuz fetches the profile and generates a digital card
3. Card displays name, title, company, and contact links
4. QR code links directly to the card for easy sharing
5. "Save Contact" button downloads vCard file

## Card Data

BizBuz reads the following fields from Prof profiles:

- `name` - Display name
- `title` - Job title / role
- `company` - Organization
- `email` - Email address
- `phone` - Phone number
- `website` - Personal/company website
- `linkedin` - LinkedIn profile
- `twitter` - Twitter/X handle
- `github` - GitHub username
- `bio` - Short bio/description

## Planet Nine Ecosystem

BizBuz is part of the Planet Nine ecosystem:

- **Prof** - Profile management with PII data
- **Sessionless** - Cryptographic authentication
- **Fount** - User and storage management

## License

MIT

## Author

Planet Nine
