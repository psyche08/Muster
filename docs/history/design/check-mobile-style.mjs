#!/usr/bin/env node
// Source-token regression only; this is not a pixel, keyboard or device test.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(directory, '../..');
const tokens = JSON.parse(fs.readFileSync(path.join(directory, 'mobile-style-tokens.json'), 'utf8'));
const colors = tokens.colors;

function luminance(hex) {
  assert.match(hex, /^#[0-9A-F]{6}$/);
  const [red, green, blue] = hex.slice(1).match(/../g).map(channel => {
    const value = parseInt(channel, 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrast(foreground, background) {
  const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

// Check every standard text role on the surfaces on which it may be rendered.
const pairs = ['background', 'surface', 'raised'].flatMap(background =>
  ['text', 'secondary', 'muted', 'accent'].map(foreground => [foreground, background]));
pairs.push(['onPrimary', 'primaryFill'], ['success', 'surface'],
  ['warning', 'warningSurface'], ['danger', 'dangerSurface'], ['text', 'userBubble']);
for (const [foreground, background] of pairs) {
  const ratio = contrast(colors[foreground], colors[background]);
  assert.ok(ratio >= 4.5, `${foreground} on ${background}: ${ratio.toFixed(2)} < 4.5`);
}
assert.equal(contrast('#000000', '#FFFFFF'), 21);
assert.equal(contrast('#161B22', '#161B22'), 1);
assert.deepEqual(tokens.spacing, [4, 8, 12, 16, 24, 32]);
for (const [platform, minimum] of Object.entries(tokens.minimumTouchTarget)) {
  assert.ok(minimum >= 44, `${platform} touch target is too small`);
}
const viewport = tokens.conversationViewport;
for (const fraction of Object.values(viewport)) assert.ok(fraction > 0 && fraction < 1);
assert.ok(viewport.headerMaxFraction + viewport.composerMaxFraction
  + viewport.messageReservedFraction <= 1, 'Fixed conversation regions must leave space for messages');
console.log(`PASS: ${pairs.length} standard foreground/background contrast pairs (>= 4.5:1)`);

// Parse declarations, not references or comments. The platform files deliberately
// keep literal semantic colors so changes can be checked without an OS runtime.
function readDeclarations(source) {
  const declarations = new Map();
  for (const line of source.split('\n')) {
    const declaration = line.match(/^\s*(?:(?:private|public|internal)\s+)?(?:(?:static\s+(?:readonly|let|var))|val)\s+(\w+)(?:\s*:\s*\w+)?\s*=\s*(.+)$/);
    if (!declaration) continue;
    const hex = declaration[2].match(/(?:#|0x)(?:FF)?([0-9A-Fa-f]{6})(?![0-9A-Fa-f])/);
    if (hex) declarations.set(declaration[1].toLowerCase(), `#${hex[1].toUpperCase()}`);
  }
  return declarations;
}

assert.equal(readDeclarations('static let background = Color(hex: 0x0D1117)').get('background'), colors.background);
assert.equal(readDeclarations('val Background = Color(0xFF0D1117)').get('background'), colors.background);
assert.equal(readDeclarations("static readonly background: string = '#0D1117';").get('background'), colors.background);
assert.equal(readDeclarations('// static let background = Color(hex: 0xFFFFFF)').size, 0);

// The shell contract is a presentation contract only. Destination keys name
// pages; they never encode a capability, an authorization or a reachability.
const shell = tokens.shell;
assert.deepEqual(shell.destinations, ['sessions', 'terminal', 'devices', 'settings']);
assert.deepEqual(Object.keys(shell.labels).sort(), [...shell.destinations].sort());
for (const label of Object.values(shell.labels)) assert.ok(label.trim().length > 0);

// Half-folded (flex) geometry must leave both halves usable. A posture that
// cannot satisfy these minimums falls back to a single-pane layout instead of
// splitting the transcript or the composer into an unusable strip.
const layout = tokens.layout;
for (const value of Object.values(layout)) assert.ok(Number.isFinite(value) && value > 0);
assert.ok(layout.flexMinTopFraction < layout.flexDefaultTopFraction);
assert.ok(layout.flexDefaultTopFraction < layout.flexMaxTopFraction);
assert.ok(layout.flexMaxTopFraction < 1);
assert.ok(layout.flexMinHeight * layout.flexMinTopFraction >= layout.flexMinTranscript,
  'The smallest accepted flex height must still fit the transcript minimum');
assert.ok(layout.flexMinHeight * (1 - layout.flexMaxTopFraction) >= layout.flexMinComposer,
  'The lowest crease must still leave the composer and its approvals usable');
console.log(`PASS: shell contract (${shell.destinations.length} destinations, ${Object.keys(layout).length} layout constants)`);

function readNumbers(source) {
  const declarations = new Map();
  for (const line of source.split('\n')) {
    const declaration = line.match(/^\s*(?:(?:private|public|internal)\s+)?(?:(?:static\s+(?:readonly|let|var|const))|(?:const\s+)?val)\s+(\w+)(?:\s*:\s*[\w<>.]+)?\s*=\s*(-?[\d.]+)f?;?\s*$/);
    if (!declaration) continue;
    const value = Number(declaration[2]);
    if (Number.isFinite(value)) declarations.set(declaration[1].toLowerCase(), value);
  }
  return declarations;
}

assert.equal(readNumbers('static let flexMinHeight: CGFloat = 600').get('flexminheight'), 600);
assert.equal(readNumbers('  static readonly flexMinHeight: number = 600;').get('flexminheight'), 600);
assert.equal(readNumbers('const val FlexMaxTopFraction = 0.62f').get('flexmaxtopfraction'), 0.62);
assert.equal(readNumbers('export const FLEX_MIN_HEIGHT: number = 480;').size, 0);

const separator = process.argv.indexOf('--layout');
const themeFiles = process.argv.slice(2, separator < 0 ? undefined : separator);
const layoutFiles = separator < 0 ? [] : process.argv.slice(separator + 1);
assert.ok(themeFiles.length > 0, 'Pass the iOS, HarmonyOS and Android theme source paths to check their role values.');
for (const relative of themeFiles) {
  const declarations = readDeclarations(fs.readFileSync(path.resolve(root, relative), 'utf8'));
  for (const [role, hex] of Object.entries(colors)) {
    assert.equal(declarations.get(role.toLowerCase()), hex, `${relative}: ${role} must be ${hex}`);
  }
  console.log(`PASS: ${relative} (${Object.keys(colors).length} semantic colors)`);
}
for (const relative of layoutFiles) {
  const source = fs.readFileSync(path.resolve(root, relative), 'utf8');
  const declarations = readNumbers(source);
  for (const [name, value] of Object.entries(layout)) {
    assert.equal(declarations.get(name.toLowerCase()), value, `${relative}: ${name} must be ${value}`);
  }
  for (const destination of shell.destinations) {
    assert.ok(new RegExp(`\\b${destination}\\b`, 'i').test(source),
      `${relative}: the ${destination} destination must exist`);
  }
  console.log(`PASS: ${relative} (${Object.keys(layout).length} layout constants, ${shell.destinations.length} destinations)`);
}
