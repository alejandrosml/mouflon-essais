//! Fichiers projetés en mémoire, en lecture seule.
//!
//! Les images d'un animal gardées sur le disque (voir le cache de
//! `mouflon-render`) se lisent ainsi sans copie : leurs pages viennent du
//! fichier à la demande et restent « propres ». Elles ne comptent pas dans
//! l'empreinte mémoire de l'app, et le système peut les reprendre quand il
//! manque de mémoire, puis les relire du disque au besoin.

use std::path::Path;
use std::sync::Arc;

/// Octets d'un fichier, partagés.
pub type SharedBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// Le fichier entier, projeté en mémoire (sur les systèmes Unix) ou lu.
///
/// Le fichier ne doit pas être raccourci tant que la projection vit : le
/// cache de Mouflon remplace ses fichiers par renommage, ce qui laisse
/// l'ancien intact pour qui le lit encore.
///
/// # Errors
///
/// Le fichier n'existe pas, ne se lit pas, ou ne se projette pas.
pub fn map_file(path: &Path) -> std::io::Result<SharedBytes> {
    #[cfg(unix)]
    {
        Ok(Arc::new(Mapped::open(path)?))
    }
    #[cfg(not(unix))]
    {
        Ok(Arc::new(std::fs::read(path)?))
    }
}

/// Une projection en lecture seule.
#[cfg(unix)]
struct Mapped {
    ptr: *mut libc::c_void,
    len: usize,
}

// SAFETY: la projection est en lecture seule et n'appartient qu'à cette
// valeur, qui la défait une seule fois (`Drop`) : la lire depuis plusieurs
// fils à la fois est sûr.
#[cfg(unix)]
unsafe impl Send for Mapped {}
// SAFETY: voir `Send` ci-dessus ; rien ne l'écrit.
#[cfg(unix)]
unsafe impl Sync for Mapped {}

#[cfg(unix)]
impl Mapped {
    fn open(path: &Path) -> std::io::Result<Self> {
        use std::os::fd::AsRawFd;

        let file = std::fs::File::open(path)?;
        let len = usize::try_from(file.metadata()?.len())
            .map_err(|_| std::io::Error::other("fichier trop grand"))?;
        if len == 0 {
            return Ok(Self {
                ptr: std::ptr::null_mut(),
                len: 0,
            });
        }
        // SAFETY: projection en lecture seule et privée d'un descripteur
        // ouvert, sur toute sa longueur ; le résultat est vérifié juste
        // après. La projection survit à la fermeture du fichier.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { ptr, len })
    }
}

#[cfg(unix)]
impl AsRef<[u8]> for Mapped {
    fn as_ref(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: `ptr` désigne `len` octets projetés en lecture, valables
        // jusqu'au `munmap` de `Drop`, qui ne peut pas avoir eu lieu tant
        // que `self` est emprunté.
        unsafe { std::slice::from_raw_parts(self.ptr.cast::<u8>(), self.len) }
    }
}

#[cfg(unix)]
impl Drop for Mapped {
    fn drop(&mut self) {
        if self.len > 0 {
            // SAFETY: la projection faite par `open`, défaite une seule fois.
            unsafe {
                libc::munmap(self.ptr, self.len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mapped_file_reads_like_the_file_and_outlives_its_replacement() {
        let dir = std::env::temp_dir().join(format!("mouflon-projection-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("images");
        let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(&path, &data).unwrap();
        let mapped = map_file(&path).unwrap();
        assert_eq!((*mapped).as_ref(), data.as_slice());
        // Remplacé par renommage, comme le fait le cache : la projection garde l'ancien.
        let other = dir.join("autre");
        std::fs::write(&other, b"nouveau").unwrap();
        std::fs::rename(&other, &path).unwrap();
        assert_eq!((*mapped).as_ref(), data.as_slice());
        std::fs::write(dir.join("vide"), b"").unwrap();
        assert!((*map_file(&dir.join("vide")).unwrap()).as_ref().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
