import { Keypair } from '@stellar/stellar-sdk';
import { encryptSecretKey, decryptSecretKey, EncryptedSecret } from './cryptoUtils';
import { query } from './db';

export interface UserWallet {
  publicKey: string;
  encryptedSecret: EncryptedSecret;
  createdAt: number;
}

/**
 * Creates a new custodial Stellar keypair for a verified user and encrypts secret key.
 */
export async function createCustodialWallet(phoneHash: string): Promise<UserWallet> {
  const pair = Keypair.random();
  const publicKey = pair.publicKey();
  const secretKey = pair.secret();

  const encryptedSecret = encryptSecretKey(secretKey);
  const createdAt = Date.now();

  const wallet: UserWallet = {
    publicKey,
    encryptedSecret,
    createdAt
  };

  await query(
    `INSERT INTO whatsapp_wallets
       (phone_hash, public_key, encrypted_data, iv, tag, created_at, wrapped_dek, dek_iv, dek_tag, key_version)
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6 / 1000.0), $7, $8, $9, $10)
     ON CONFLICT (phone_hash) DO NOTHING`,
    [
      phoneHash,
      publicKey,
      encryptedSecret.encryptedData,
      encryptedSecret.iv,
      encryptedSecret.tag,
      createdAt,
      encryptedSecret.wrappedDek,
      encryptedSecret.dekIv,
      encryptedSecret.dekTag,
      encryptedSecret.keyVersion
    ]
  );

  return wallet;
}

/**
 * Retrieves a user's wallet info (public key and encrypted secret).
 */
export async function getWallet(phoneHash: string): Promise<UserWallet | undefined> {
  const res = await query('SELECT * FROM whatsapp_wallets WHERE phone_hash = $1', [phoneHash]);
  if (res.rows.length === 0) return undefined;

  const row = res.rows[0];
  return {
    publicKey: row.public_key,
    encryptedSecret: {
      encryptedData: row.encrypted_data,
      iv: row.iv,
      tag: row.tag,
      // NULL for rows written before envelope encryption (#944).
      wrappedDek: row.wrapped_dek ?? undefined,
      dekIv: row.dek_iv ?? undefined,
      dekTag: row.dek_tag ?? undefined,
      keyVersion: row.key_version ?? undefined
    },
    createdAt: new Date(row.created_at).getTime()
  };
}

/**
 * Decrypts secret key for transaction execution. Key is never saved in plaintext or output to chat.
 */
export async function getDecryptedSecretKey(phoneHash: string): Promise<string | null> {
  const wallet = await getWallet(phoneHash);
  if (!wallet) return null;

  return decryptSecretKey(
    wallet.encryptedSecret.encryptedData,
    wallet.encryptedSecret.iv,
    wallet.encryptedSecret.tag,
    wallet.encryptedSecret
  );
}
