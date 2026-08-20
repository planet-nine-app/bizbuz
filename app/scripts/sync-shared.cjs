#!/usr/bin/env node
// Copies shared/vcard.js (the vCard format shared with the legacy server.js)
// into src/lib/, so the frontend can import it as a local module without a
// bundler needing to resolve paths outside src/.

'use strict';

const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const src = path.join(ROOT, '..', 'shared', 'vcard.js');
const destDir = path.join(ROOT, 'src', 'lib');
const dest = path.join(destDir, 'vcard.js');

fs.mkdirSync(destDir, { recursive: true });
fs.copyFileSync(src, dest);
console.log(`Synced ${path.relative(ROOT, src)} -> ${path.relative(ROOT, dest)}`);
