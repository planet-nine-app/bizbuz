# BizBuz - Development Documentation

## Overview

BizBuz is Planet Nine's digital business card service that creates beautiful, shareable business cards from Prof profiles. Users can share via QR codes and let contacts save their info with one tap.

**Location**: `/planet-nine/bizbuz/`
**Port**: 3011 (default)
**Status**: Initial Implementation (November 2025)

## Architecture

### Core Components

1. **Express Server** (`server.js`) - Main application server
2. **Prof Integration** - Fetches user profiles from Prof service
3. **Card Renderer** - Generates HTML business card pages
4. **vCard Generator** - Creates downloadable contact files
5. **QR Code Generator** - Creates scannable QR code images

### Data Flow

```
User Request → BizBuz → Prof Service → Profile Data → HTML Card
                                                   ↓
                                             vCard Download
                                                   ↓
                                              QR Code Image
```

## Implementation Status

### Completed (November 2025)

- Express server with static file serving
- Prof integration with fallback demo profile
- Business card HTML generation with Planet Nine theme
- vCard 3.0 download endpoint
- QR code generation endpoint
- Landing page (`public/index.html`)
- Documentation (README.md, CLAUDE.md)

### TODO

- Real Prof service testing
- Sessionless authentication for private profiles
- Custom card themes
- Card analytics (privacy-preserving)
- Profile image support
- Social share previews (Open Graph)

## API Endpoints

### GET /

Landing page with feature overview and demo link.

### GET /card/:identifier

Renders HTML business card page.

**Parameters:**
- `identifier` - User UUID, pubKey, or 'demo'

**Response:** HTML page with card display, vCard download button, QR code

### GET /vcard/:identifier

Downloads vCard file for contact saving.

**Parameters:**
- `identifier` - User UUID, pubKey, or 'demo'

**Response:** `text/vcard` file attachment

**vCard Format:** Version 3.0 with:
- FN (Full Name)
- TITLE
- ORG (Organization)
- TEL (Phone)
- EMAIL
- URL (Website, LinkedIn, GitHub, Twitter)
- NOTE (Bio)

### GET /qr/:identifier

Generates QR code PNG image.

**Parameters:**
- `identifier` - User UUID, pubKey, or 'demo'

**Response:** PNG image (300x300px)

## Configuration

### Environment Variables

```bash
PORT=3011                              # Server port
PROF_BASE_URL=http://localhost:3012    # Prof service URL
```

### Default Prof URL

If `PROF_BASE_URL` is not set, defaults to `http://localhost:3012`.

## Prof Integration

### Profile Fetch

```javascript
const response = await fetch(`${PROF_BASE_URL}/profile/${identifier}`);
const profile = await response.json();
```

### Expected Profile Structure

```javascript
{
  name: "Jane Developer",
  title: "Software Engineer",
  company: "Tech Corp",
  email: "jane@example.com",
  phone: "+1 (555) 123-4567",
  website: "https://jane.dev",
  linkedin: "https://linkedin.com/in/jane",
  twitter: "janedev",
  github: "janedev",
  bio: "Building cool stuff"
}
```

### Demo Profile

When Prof is unavailable or identifier is 'demo':

```javascript
{
  name: "Demo User",
  title: "Software Engineer",
  company: "Planet Nine",
  email: "demo@planetnine.app",
  phone: "+1 (555) 123-4567",
  website: "https://planetnine.app",
  github: "planet-nine",
  bio: "This is a demo business card..."
}
```

## Card Design

### Theme

- **Background**: Dark gradient (#0a0a0f → #1a1a2e → #16213e)
- **Primary Color**: Planet Nine green (#10b981)
- **Secondary Colors**: Blue (#3b82f6), Purple (#8b5cf6)
- **Text**: Light gray (#e0e0e0, #9ca3af)

### Visual Elements

- Floating particle animation
- Gradient avatar with initials
- Glowing border effects
- Responsive layout (mobile-friendly)

### Card Sections

1. **Header** - Avatar, name, title, company
2. **Contact Links** - Email, phone, website with icons
3. **Social Links** - LinkedIn, Twitter, GitHub
4. **Bio** - Short description
5. **Actions** - Save Contact button, QR code toggle

## vCard Generation

### Format

```
BEGIN:VCARD
VERSION:3.0
FN:Jane Developer
TITLE:Software Engineer
ORG:Tech Corp
TEL;TYPE=WORK,VOICE:+1 (555) 123-4567
EMAIL;TYPE=WORK:jane@example.com
URL;TYPE=WORK:https://jane.dev
URL;TYPE=LinkedIn:https://linkedin.com/in/jane
URL;TYPE=GitHub:https://github.com/janedev
NOTE:Building cool stuff
END:VCARD
```

### Filename

`{name}-bizbuz.vcf` (spaces replaced with dashes)

## QR Code Generation

Uses `qrcode` npm package:

```javascript
const QRCode = require('qrcode');
const cardUrl = `${baseUrl}/card/${identifier}`;
const qrBuffer = await QRCode.toBuffer(cardUrl, {
  width: 300,
  margin: 2,
  color: { dark: '#10b981', light: '#0a0a0f' }
});
```

## File Structure

```
bizbuz/
├── server.js           # Main Express server
├── package.json        # Dependencies
├── README.md           # User documentation
├── CLAUDE.md           # Development documentation (this file)
└── public/
    └── index.html      # Landing page
```

## Dependencies

```json
{
  "express": "^4.18.2",
  "qrcode": "^1.5.3",
  "sessionless-node": "latest"
}
```

## Development

### Start Server

```bash
npm start          # Production
npm run dev        # Development with nodemon
```

### Test Endpoints

```bash
# Landing page
curl http://localhost:3011/

# Demo card
curl http://localhost:3011/card/demo

# Download vCard
curl -O http://localhost:3011/vcard/demo

# Generate QR code
curl -o qr.png http://localhost:3011/qr/demo
```

## Future Enhancements

### Priority 1: Prof Integration

- Test with real Prof service
- Handle authentication for private profiles
- Support profile images (base64)

### Priority 2: Customization

- Multiple card themes (light, dark, colorful)
- Custom color schemes
- Profile image upload
- Social share previews

### Priority 3: Features

- Card analytics (view counts, privacy-preserving)
- Short URL generation (bizbuz.app/u/jane)
- NFC tag support for physical cards
- Batch card generation for teams

### Priority 4: Production

- SSL/TLS configuration
- Rate limiting
- Caching for Prof data
- Error tracking

## Related Services

- **Prof** (Port 3012) - Profile management
- **Fount** - User authentication
- **BDO** - Data storage
- **Glyphenge** - Link tapestries (similar concept)

## Last Updated

November 22, 2025 - Initial implementation with Prof integration, business card generation, vCard download, and QR code generation.
