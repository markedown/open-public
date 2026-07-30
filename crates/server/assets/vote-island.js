// Anonymous voting island. Blinds a per-poll token, gets it blind-signed by the
// server (which never sees it), and spends it anonymously to cast a ballot.
// See docs/anonymous-voting.md. Pure JS, no WASM.
import { RSABSSA } from '@cloudflare/blindrsa-ts';

const suite = RSABSSA.SHA384.PSS.Randomized();

const b64 = (u) => btoa(String.fromCharCode(...new Uint8Array(u)));
const b64url = (u) => b64(u).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
const fromB64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const tokenKey = (slug) => `op-vote:${slug}`;

async function importPubKey(spkiB64) {
  return crypto.subtle.importKey(
    'spki', fromB64(spkiB64), { name: 'RSA-PSS', hash: 'SHA-384' }, true, ['verify']
  );
}

// Get (from cache) or mint this poll's anonymous token. A minted token is kept
// in localStorage so a reload reuses it: the account can be issued only one, so
// losing it before casting would strand the vote.
async function ensureToken(el) {
  const { pollSlug: slug, pollCountry: country, pollPubkey: pubB64 } = el.dataset;
  const cached = localStorage.getItem(tokenKey(slug));
  if (cached) return JSON.parse(cached);

  const pub = await importPubKey(pubB64);
  const token = crypto.getRandomValues(new Uint8Array(32));
  const prepared = suite.prepare(token);
  const { blindedMsg, inv } = await suite.blind(pub, prepared);

  const res = await fetch(`/${country}/poll/${slug}/token`, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: 'blinded=' + encodeURIComponent(b64(blindedMsg)),
  });
  if (res.status === 409) return { unavailable: true };
  if (!res.ok) return null;

  const blindSig = fromB64((await res.text()).trim());
  const sig = await suite.finalize(pub, prepared, blindSig, inv);
  const rec = {
    token: b64url(token),
    signature: b64url(sig),
    randomizer: b64url(prepared.slice(0, 32)),
  };
  localStorage.setItem(tokenKey(slug), JSON.stringify(rec));
  return rec;
}

async function cast(el, optionIds) {
  const { pollSlug: slug, pollCountry: country } = el.dataset;
  const rec = await ensureToken(el);
  if (!rec) return { ok: false };
  if (rec.unavailable) return { ok: false, unavailable: true };

  // token/signature/randomizer are already url-safe base64; option ids are
  // integers, so the body needs no percent-encoding.
  const raw =
    `token=${rec.token}&signature=${rec.signature}` +
    (rec.randomizer ? `&randomizer=${rec.randomizer}` : '') +
    optionIds.map((id) => `&option_id=${id}`).join('');

  const res = await fetch(`/${country}/poll/${slug}/cast`, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: raw,
    redirect: 'manual',
  });
  return { ok: res.ok || res.status === 0 || res.type === 'opaqueredirect' };
}

function enhance(el) {
  const form = el.querySelector('[data-vote]');
  if (!form) return;
  const slug = el.dataset.pollSlug;
  const status = el.querySelector('[data-vote-status]');
  el.hidden = false;

  // The server cannot tell whether this account voted (that is the anonymity),
  // so whether you have voted is remembered here, on your own device. If you
  // have, show the note and do not offer the controls again.
  if (localStorage.getItem(`op-voted:${slug}`)) {
    form.hidden = true;
    if (status) {
      status.hidden = false;
      status.textContent = el.dataset.msgVoted;
      status.classList.remove('text-red-600');
      status.classList.add('text-ink-muted');
    }
    return;
  }

  // Warm the token as soon as the page is ready, so casting is instant and the
  // token request and the vote are separated in time.
  ensureToken(el).catch(() => {});

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const submitter = e.submitter;
    let ids = [];
    if (submitter && submitter.name === 'option') {
      ids = [submitter.value];
    } else {
      ids = [...form.querySelectorAll('input[name="option"]:checked')].map((i) => i.value);
    }
    if (ids.length === 0) return;
    form.querySelectorAll('button').forEach((b) => (b.disabled = true));
    const out = await cast(el, ids);
    if (out.ok) {
      localStorage.setItem(`op-voted:${slug}`, '1');
      location.reload();
    } else if (status) {
      status.hidden = false;
      status.textContent = out.unavailable
        ? el.dataset.msgUnavailable
        : el.dataset.msgError;
      form.querySelectorAll('button').forEach((b) => (b.disabled = false));
    }
  });
}

document.querySelectorAll('[data-poll-slug]').forEach(enhance);
