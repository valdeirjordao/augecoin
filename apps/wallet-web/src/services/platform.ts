// AUGECOIN Wallet Web — platform identity (Camada 1).
//
// The platform account (email/name/profile/preferences/session) lives on the
// wallet-web backend. Registering creates ONLY a platform identity plus a
// *hidden* blockchain keypair whose mnemonic is encrypted client-side (IndexedDB
// keystore) with the member's password. No AUGEID is ever created here — the
// blockchain wallet only becomes active when an AUGEID is bought or gifted.

import { generateMnemonic } from 'bip39';
import { derivePublicKeyHex } from './crypto';
import * as keystore from '../lib/keystore';
import { apiFetch } from './api';
import type { DirectoryMember, PlatformUser, UserPreferences } from '../types';

export interface RegisteredMember {
  user: PlatformUser;
  mnemonic: string;
}

function encodeMnemonic(mnemonic: string): Uint8Array {
  return new TextEncoder().encode(mnemonic);
}

function decodeMnemonic(data: Uint8Array): string {
  return new TextDecoder().decode(data);
}

function keystoreId(email: string): string {
  return `member:${email.trim().toLowerCase()}`;
}

/** Register a platform member. Creates identity + encrypted keypair, no AUGEID. */
export async function registerMember(args: {
  name: string;
  email: string;
  password: string;
}): Promise<RegisteredMember> {
  const email = args.email.trim().toLowerCase();

  // Generate the blockchain identity keypair client-side (never sent anywhere).
  const mnemonic = generateMnemonic(128);
  const publicKeyHex = await derivePublicKeyHex(mnemonic, 0);

  const { user } = await apiFetch<{ user: PlatformUser }>('/auth/register', {
    method: 'POST',
    body: { email, password: args.password, display_name: args.name.trim(), public_key_hex: publicKeyHex },
  });

  await keystore.saveEncrypted(keystoreId(email), encodeMnemonic(mnemonic), args.password);
  return { user, mnemonic };
}

/** Login: authenticate on the backend and decrypt the keypair client-side. */
export async function loginMember(email: string, password: string): Promise<RegisteredMember> {
  const clean = email.trim().toLowerCase();
  const { user } = await apiFetch<{ user: PlatformUser }>('/auth/login', {
    method: 'POST',
    body: { email: clean, password },
  });

  const data = await keystore.loadEncrypted(keystoreId(clean), password);
  if (!data) throw new Error('Chave local não encontrada. Reimporte a carteira.');

  const mnemonic = decodeMnemonic(data);
  const derivedPublicKey = await derivePublicKeyHex(mnemonic, 0);
  if (!user.public_key_hex) {
    const updated = await updatePublicKey(derivedPublicKey);
    return { user: updated, mnemonic };
  }
  if (user.public_key_hex.toLowerCase() !== derivedPublicKey.toLowerCase()) {
    throw new Error('A chave pública da conta não corresponde à wallet local.');
  }

  return { user, mnemonic };
}

async function updatePublicKey(publicKeyHex: string): Promise<PlatformUser> {
  const { user } = await apiFetch<{ user: PlatformUser }>('/me/key', {
    method: 'PATCH',
    body: { public_key_hex: publicKeyHex },
    csrf: true,
  });
  return user;
}

/** Restore the platform session (user + preferences) from the HttpOnly cookie. */
export async function restoreSession(): Promise<{
  user: PlatformUser;
  preferences: UserPreferences;
} | null> {
  try {
    return await apiFetch<{ user: PlatformUser; preferences: UserPreferences }>('/me');
  } catch {
    return null;
  }
}

/** Global logout — revokes every refresh token for the user. */
export async function logoutMember(): Promise<void> {
  try {
    await apiFetch('/auth/logout', { method: 'POST' });
  } catch {
    /* ignore network errors on logout */
  }
}

export async function updateProfile(displayName: string): Promise<PlatformUser> {
  const { user } = await apiFetch<{ user: PlatformUser }>('/profile', {
    method: 'PATCH',
    body: { display_name: displayName },
    csrf: true,
  });
  return user;
}

export async function updatePreferences(
  prefs: Partial<UserPreferences>,
): Promise<UserPreferences> {
  const { preferences } = await apiFetch<{ preferences: UserPreferences }>('/preferences', {
    method: 'PATCH',
    body: prefs,
    csrf: true,
  });
  return preferences;
}

/** Resolve the full member directory (for gift/transfer recipient selection). */
export async function directory(): Promise<DirectoryMember[]> {
  const res = await apiFetch<{ members: DirectoryMember[] }>('/directory');
  return res.members;
}

/** Resolve a seller's display name from their on-chain public key. */
export async function displayNameByKey(publicKeyHex: string): Promise<string | null> {
  try {
    const res = await apiFetch<{ display_name: string | null }>(
      `/directory/by-key/${encodeURIComponent(publicKeyHex)}`,
    );
    return res.display_name;
  } catch {
    return null;
  }
}
