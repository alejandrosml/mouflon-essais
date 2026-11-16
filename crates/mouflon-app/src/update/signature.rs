//! La signature d'une version : la preuve qu'elle vient de l'auteur.
//!
//! Chaque version sort avec la liste de ses fichiers (`release.rs`) et la
//! signature de cette liste, faite sur le Mac de l'auteur avec sa clé
//! (`ssh-keygen -Y sign`, espace de noms `mouflon`). Mouflon porte la clé
//! publique : une liste qui n'est pas signée par elle n'est pas lue, et rien
//! de ce qu'elle annonce n'est installé.
//!
//! Le format est celui d'OpenSSH (`PROTOCOL.sshsig`) : sous son armure en
//! base64, la signature dit la clé, l'espace de noms et la fonction de
//! hachage, et signe « SSHSIG », l'espace de noms, un champ réservé, le nom de
//! la fonction et le haché du message. On n'accepte qu'une clé Ed25519 et
//! SHA-512, ce que fait `ssh-keygen` sans qu'on lui dise rien.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_compact::{PublicKey, Signature, sha512};

/// L'espace de noms des signatures de Mouflon : une signature faite pour
/// autre chose avec la même clé ne vaut pas pour une version.
pub const NAMESPACE: &str = "mouflon";

const MAGIC: &[u8] = b"SSHSIG";
const KEY_TYPE: &[u8] = b"ssh-ed25519";
const HASH: &[u8] = b"sha512";
const BEGIN: &str = "-----BEGIN SSH SIGNATURE-----";
const END: &str = "-----END SSH SIGNATURE-----";

/// Pourquoi une signature n'est pas reçue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignatureError {
    #[error("la clé publique ne se lit pas")]
    Key,
    #[error("la signature ne se lit pas")]
    Unreadable,
    #[error("la signature n'est pas de la bonne sorte (clé Ed25519, SHA-512, espace « mouflon »)")]
    Kind,
    #[error("la signature a été faite avec une autre clé")]
    OtherKey,
    #[error("la signature ne correspond pas au texte")]
    Mismatch,
}

/// Vérifie que `message` a été signé par la clé `public_key` (une ligne
/// « ssh-ed25519 AAAA… », comme dans un fichier `.pub`) : `signature` est le
/// fichier `.sig` qu'écrit `ssh-keygen -Y sign -n mouflon`.
pub fn verify(public_key: &str, message: &[u8], signature: &str) -> Result<(), SignatureError> {
    use SignatureError::{Key, Kind, Mismatch, OtherKey, Unreadable};

    let key = public_key_blob(public_key)?;
    let raw = raw_key(&key).ok_or(Key)?;

    let bytes = unarmor(signature)?;
    let mut fields = Reader(&bytes);
    if fields.take(MAGIC.len()) != Some(MAGIC) || fields.u32() != Some(1) {
        return Err(Unreadable);
    }
    let signer = fields.string().ok_or(Unreadable)?;
    let namespace = fields.string().ok_or(Unreadable)?;
    let reserved = fields.string().ok_or(Unreadable)?;
    let hash = fields.string().ok_or(Unreadable)?;
    let mut signed = Reader(fields.string().ok_or(Unreadable)?);
    let kind = signed.string().ok_or(Unreadable)?;
    let proof = signed.string().ok_or(Unreadable)?;
    if !fields.done() || !signed.done() {
        return Err(Unreadable);
    }
    if namespace != NAMESPACE.as_bytes() || hash != HASH || kind != KEY_TYPE {
        return Err(Kind);
    }
    if signer != key.as_slice() {
        return Err(OtherKey);
    }

    // Ce qui est signé n'est pas le message, mais ceci.
    let digest = sha512::Hash::hash(message);
    let mut blob = MAGIC.to_vec();
    for field in [namespace, reserved, hash, digest.as_slice()] {
        blob.extend_from_slice(&(field.len() as u32).to_be_bytes());
        blob.extend_from_slice(field);
    }
    let key = PublicKey::from_slice(raw).map_err(|_| Key)?;
    let proof = Signature::from_slice(proof).map_err(|_| Unreadable)?;
    key.verify(&blob, &proof).map_err(|_| Mismatch)
}

/// La clé d'une ligne « ssh-ed25519 AAAA… [commentaire] », telle qu'OpenSSH
/// l'écrit dans une signature.
fn public_key_blob(line: &str) -> Result<Vec<u8>, SignatureError> {
    let mut words = line.split_whitespace();
    if words.next().map(str::as_bytes) != Some(KEY_TYPE) {
        return Err(SignatureError::Key);
    }
    let blob = words
        .next()
        .and_then(|b| STANDARD.decode(b).ok())
        .ok_or(SignatureError::Key)?;
    raw_key(&blob).ok_or(SignatureError::Key)?;
    Ok(blob)
}

/// Les trente-deux octets de la clé, dans son enveloppe.
fn raw_key(blob: &[u8]) -> Option<&[u8]> {
    let mut fields = Reader(blob);
    let kind = fields.string()?;
    let key = fields.string()?;
    (kind == KEY_TYPE && key.len() == PublicKey::BYTES && fields.done()).then_some(key)
}

/// Les octets d'une signature, sans son armure.
fn unarmor(signature: &str) -> Result<Vec<u8>, SignatureError> {
    let mut lines = signature.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next() != Some(BEGIN) {
        return Err(SignatureError::Unreadable);
    }
    let mut body = String::new();
    let mut closed = false;
    for line in lines {
        if closed {
            // Rien après la fin.
            return Err(SignatureError::Unreadable);
        }
        if line == END {
            closed = true;
        } else {
            body.push_str(line);
        }
    }
    if !closed {
        return Err(SignatureError::Unreadable);
    }
    STANDARD
        .decode(body)
        .map_err(|_| SignatureError::Unreadable)
}

/// Lit les champs d'un message d'OpenSSH : des entiers de quatre octets, gros
/// bout devant, et des chaînes précédées de leur longueur.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.0.len() {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Option<&'a [u8]> {
        let n = usize::try_from(self.u32()?).ok()?;
        self.take(n)
    }

    fn done(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Une vraie signature, faite par `ssh-keygen -Y sign -n mouflon` (OpenSSH
    // 9.6) avec une clé d'essai : la clé privée n'existe plus.
    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO8IgUfcdPG/3t07v2Por7d8cg0lkJGIySZftby/cG5c Mouflon";
    const MESSAGE: &[u8] = b"mouflon 1\nversion 1.0.5\n";
    const SIGNATURE: &str = "-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAg7wiBR9x08b/e3Tu/Y+ivt3xyDS
WQkYjJJl+1vL9wblwAAAAHbW91ZmxvbgAAAAAAAAAGc2hhNTEyAAAAUwAAAAtzc2gtZWQy
NTUxOQAAAEB5IGNhZchfjpSXx79e9NfHKYqBOFW1FYrmwl8W0yofbs24CkygZaY3Lm6XkB
yt80tmWAjS0sCQOBb1IitKuUsO
-----END SSH SIGNATURE-----
";
    // Une autre clé d'essai, qui n'a rien signé.
    const OTHER_KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILBM3oF6KrdJ6bu8ASYewhUcWz1ZquO51kldsc0ijpGa";

    /// La signature, avec un champ changé : `edit` reçoit ses octets.
    fn forged(edit: impl FnOnce(&mut Vec<u8>)) -> String {
        let mut bytes = unarmor(SIGNATURE).unwrap();
        edit(&mut bytes);
        format!("{BEGIN}\n{}\n{END}\n", STANDARD.encode(bytes))
    }

    #[test]
    fn a_signature_made_by_ssh_keygen_is_accepted() {
        assert_eq!(verify(KEY, MESSAGE, SIGNATURE), Ok(()));
    }

    #[test]
    fn the_key_may_come_without_its_comment_and_the_signature_with_windows_line_ends() {
        let key = KEY.rsplit_once(' ').unwrap().0;
        assert_eq!(
            verify(key, MESSAGE, &SIGNATURE.replace('\n', "\r\n")),
            Ok(())
        );
    }

    #[test]
    fn a_changed_message_is_refused() {
        assert_eq!(
            verify(KEY, b"mouflon 1\nversion 9.9.9\n", SIGNATURE),
            Err(SignatureError::Mismatch)
        );
        assert_eq!(verify(KEY, b"", SIGNATURE), Err(SignatureError::Mismatch));
    }

    #[test]
    fn a_signature_made_with_another_key_is_refused() {
        assert_eq!(
            verify(OTHER_KEY, MESSAGE, SIGNATURE),
            Err(SignatureError::OtherKey)
        );
    }

    #[test]
    fn a_signature_that_names_our_key_without_being_made_by_it_is_refused() {
        // Quelqu'un écrit notre clé dans sa signature : le nom ne suffit pas.
        let ours = public_key_blob(OTHER_KEY).unwrap();
        let theirs = public_key_blob(KEY).unwrap();
        let signature = forged(|bytes| {
            let at = bytes
                .windows(theirs.len())
                .position(|w| w == theirs)
                .unwrap();
            bytes[at..at + ours.len()].copy_from_slice(&ours);
        });
        assert_eq!(
            verify(OTHER_KEY, MESSAGE, &signature),
            Err(SignatureError::Mismatch)
        );
    }

    #[test]
    fn a_signature_made_for_something_else_is_refused() {
        // Même clé, autre espace de noms (« mouflox ») : pas pour une version.
        let signature = forged(|bytes| {
            let at = bytes.windows(7).position(|w| w == b"mouflon").unwrap();
            bytes[at + 6] = b'x';
        });
        assert_eq!(verify(KEY, MESSAGE, &signature), Err(SignatureError::Kind));
    }

    #[test]
    fn a_damaged_signature_is_refused_without_panicking() {
        let whole = unarmor(SIGNATURE).unwrap();
        // Coupée à chaque longueur possible.
        for n in 0..whole.len() {
            let cut = forged(|bytes| bytes.truncate(n));
            assert!(verify(KEY, MESSAGE, &cut).is_err(), "coupée à {n} octets");
        }
        // Un octet changé, n'importe où.
        for i in 0..whole.len() {
            let bent = forged(|bytes| bytes[i] ^= 0x40);
            assert!(verify(KEY, MESSAGE, &bent).is_err(), "octet {i} changé");
        }
        // Des octets en trop.
        assert!(verify(KEY, MESSAGE, &forged(|bytes| bytes.push(0))).is_err());
        // Une longueur démesurée.
        let huge = forged(|bytes| bytes[10..14].copy_from_slice(&u32::MAX.to_be_bytes()));
        assert_eq!(verify(KEY, MESSAGE, &huge), Err(SignatureError::Unreadable));
    }

    #[test]
    fn what_is_not_a_signature_is_refused() {
        for text in [
            "",
            "bonjour",
            BEGIN,
            &format!("{BEGIN}\n{END}\n"),
            &format!("{BEGIN}\n!!!!\n{END}\n"),
            &SIGNATURE.replace(END, ""),
            &format!("{SIGNATURE}encore\n"),
        ] {
            assert_eq!(
                verify(KEY, MESSAGE, text),
                Err(SignatureError::Unreadable),
                "{text:?}"
            );
        }
    }

    #[test]
    fn what_is_not_an_ed25519_key_is_refused() {
        for key in [
            "",
            "ssh-ed25519",
            "ssh-ed25519 !!!",
            "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAAgQ",
            "ssh-ed25519 AAAA",
        ] {
            assert_eq!(
                verify(key, MESSAGE, SIGNATURE),
                Err(SignatureError::Key),
                "{key:?}"
            );
        }
    }
}
