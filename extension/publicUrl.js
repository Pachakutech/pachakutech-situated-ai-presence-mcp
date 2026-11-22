// Public http(s) only. The same check the host runs before the socket.
// Keep the first of each exact URL, then cap at 40.

const URL_CAP = 40;
const REF_CAP = 2000;

export function publicUrl(raw) {
  if (typeof raw !== "string" || raw.length === 0 || Array.from(raw).length > REF_CAP) return false;
  if (/[\s\p{Cc}]/u.test(raw)) return false;
  const split = raw.indexOf("://");
  if (split <= 0) return false;
  const scheme = raw.slice(0, split).toLowerCase();
  if (scheme !== "http" && scheme !== "https") return false;
  const rest = raw.slice(split + 3);
  if (rest.length === 0) return false;
  const authority = rest.split(/[/?#]/, 1)[0] ?? "";
  if (authority.length === 0 || authority.includes("@")) return false;
  const host = hostFromAuthority(authority);
  if (host === null) return false;
  return hostOk(host);
}

export function keepUrls(raw) {
  const list = Array.isArray(raw) ? raw : [];
  const kept = [];
  const seen = new Set();
  for (const item of list) {
    if (typeof item !== "string" || !publicUrl(item) || seen.has(item)) continue;
    seen.add(item);
    kept.push(item);
  }
  return { urls: kept.slice(0, URL_CAP), dropped: Math.max(0, kept.length - URL_CAP) };
}

function hostFromAuthority(authority) {
  if (authority.startsWith("[")) {
    const end = authority.indexOf("]");
    if (end <= 1) return null;
    const host = authority.slice(1, end);
    const after = authority.slice(end + 1);
    if (after.length > 0 && !/^:\d+$/.test(after)) return null;
    if (!ipv6Segments(host)) return null;
    return host;
  }
  const colon = authority.lastIndexOf(":");
  if (colon >= 0) {
    const host = authority.slice(0, colon);
    const port = authority.slice(colon + 1);
    if (host.length === 0 || !/^\d+$/.test(port)) return null;
    return host;
  }
  return authority;
}

function hostOk(host) {
  const bare = host.replace(/\.+$/, "");
  if (bare.length === 0) return false;
  if (bare.toLowerCase() === "localhost") return false;
  if (ipv4(bare)) return !blockedV4(bare);
  const v6 = ipv6Segments(bare);
  if (v6) return !blockedV6(v6);
  const parts = bare.split(".");
  if (parts.every((part) => part.length > 0 && /^\d+$/.test(part))) return false;
  return true;
}

function ipv4(host) {
  const oct = host.split(".");
  if (oct.length !== 4) return false;
  return oct.every((part) => /^(0|[1-9]\d{0,2})$/.test(part) && Number(part) <= 255);
}

function blockedV4(host) {
  const oct = host.split(".").map((part) => Number(part));
  if (oct.length !== 4 || oct.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) return true;
  const a = oct[0] ?? 0;
  const b = oct[1] ?? 0;
  const c = oct[2] ?? 0;
  const d = oct[3] ?? 0;
  return (
    a === 127 ||
    (a === 0 && b === 0 && c === 0 && d === 0) ||
    a === 10 ||
    (a === 172 && (b & 0xf0) === 16) ||
    (a === 192 && b === 168) ||
    (a === 169 && b === 254)
  );
}

function blockedV6(seg) {
  const mapped = seg[0] === 0 && seg[1] === 0 && seg[2] === 0 && seg[3] === 0 && seg[4] === 0 && seg[5] === 0xffff;
  if (mapped) {
    const a = (seg[6] ?? 0) >> 8;
    const b = (seg[6] ?? 0) & 0xff;
    const c = (seg[7] ?? 0) >> 8;
    const d = (seg[7] ?? 0) & 0xff;
    return blockedV4(`${a}.${b}.${c}.${d}`);
  }
  const loop = seg.every((n, index) => n === (index === 7 ? 1 : 0));
  const unspecified = seg.every((n) => n === 0);
  const first = seg[0] ?? 0;
  return loop || unspecified || (first & 0xfe00) === 0xfc00 || (first & 0xffc0) === 0xfe80;
}

function ipv6Segments(host) {
  let text = host;
  const dotted = text.match(/(\d+\.\d+\.\d+\.\d+)$/);
  if (dotted?.[1]) {
    const oct = dotted[1].split(".").map((part) => Number(part));
    if (oct.length !== 4 || oct.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) return null;
    const hi = ((oct[0] ?? 0) << 8) | (oct[1] ?? 0);
    const lo = ((oct[2] ?? 0) << 8) | (oct[3] ?? 0);
    text = `${text.slice(0, text.length - dotted[1].length)}${hi.toString(16)}:${lo.toString(16)}`;
  }
  const halves = text.split("::");
  if (halves.length > 2) return null;
  const left = parseSide(halves[0] ?? "");
  if (!left) return null;
  if (halves.length === 1) return left.length === 8 ? left : null;
  const right = parseSide(halves[1] ?? "");
  if (!right || left.length + right.length > 7) return null;
  return [...left, ...new Array(8 - left.length - right.length).fill(0), ...right];
}

function parseSide(side) {
  if (side.length === 0) return [];
  const parts = side.split(":");
  if (parts.some((part) => !/^[0-9a-fA-F]{1,4}$/.test(part))) return null;
  return parts.map((part) => Number.parseInt(part, 16));
}
