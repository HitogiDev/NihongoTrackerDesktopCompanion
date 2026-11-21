//! A remote file read by HTTP ranges, as `Read + Seek`, so the ZIP reader
//! of `archive` works on a CBZ on a server exactly as on one on disk: it
//! seeks to the end for the central directory, then to the one entry it was
//! asked for, and only those blocks are fetched.
//!
//! Reads go through fixed blocks kept in the [`Cache`], so the directory at
//! the end of the archive is fetched once per volume and a page shown again
//! costs no request at all.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use super::cache::Cache;
use super::client::DavError;

/// Bytes per request. A page is a few hundred KB, so most take one or two.
pub const BLOCK: u64 = 512 * 1024;

/// Fetches `len` bytes at `start`.
pub type Fetch<'a> = Box<dyn FnMut(u64, u64) -> Result<Vec<u8>, DavError> + 'a>;

pub struct RangedFile<'a> {
    fetch: Fetch<'a>,
    len: u64,
    pos: u64,
    cache: &'a Cache,
    /// Names this file's blocks in the cache (its path and signature).
    key: String,
    current: Option<(u64, Vec<u8>)>,
    /// Every block loaded while recording (`record`): what reading the
    /// archive's directory touched, for `archive::RemoteIndex`.
    recorded: Option<Vec<(u64, Arc<[u8]>)>>,
    /// What went wrong under an `io::Error`, for the caller to tell a server
    /// without ranges from a broken archive.
    pub error: Arc<Mutex<Option<DavError>>>,
}

impl<'a> RangedFile<'a> {
    pub fn new(len: u64, cache: &'a Cache, key: String, fetch: Fetch<'a>) -> Self {
        RangedFile { fetch, len, pos: 0, cache, key, current: None, recorded: None, error: Arc::new(Mutex::new(None)) }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// From now on, remembers every block loaded (`take_recorded`).
    pub fn record(&mut self) {
        self.recorded = Some(self.current.iter().map(|(index, bytes)| (index * BLOCK, Arc::from(bytes.as_slice()))).collect());
    }

    /// The blocks loaded since `record`, by offset.
    pub fn take_recorded(&mut self) -> Vec<(u64, Arc<[u8]>)> {
        self.recorded.take().unwrap_or_default()
    }

    fn block(&mut self, index: u64) -> io::Result<&[u8]> {
        if self.current.as_ref().is_none_or(|(at, _)| *at != index) {
            let key = format!("{}-b{index}", self.key);
            let bytes = match self.cache.get(&key) {
                Some(bytes) => bytes,
                None => {
                    let start = index * BLOCK;
                    let len = BLOCK.min(self.len - start);
                    match (self.fetch)(start, len) {
                        Ok(bytes) => {
                            self.cache.put(&key, &bytes);
                            bytes
                        }
                        Err(error) => {
                            let message = error.to_string();
                            *self.error.lock().unwrap() = Some(error);
                            return Err(io::Error::other(message));
                        }
                    }
                }
            };
            if let Some(recorded) = self.recorded.as_mut() {
                if !recorded.iter().any(|(at, _)| *at == index * BLOCK) {
                    recorded.push((index * BLOCK, std::sync::Arc::from(bytes.as_slice())));
                }
            }
            self.current = Some((index, bytes));
        }
        Ok(&self.current.as_ref().expect("block").1)
    }
}

impl Read for RangedFile<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let index = self.pos / BLOCK;
        let offset = (self.pos % BLOCK) as usize;
        let block = self.block(index)?;
        if offset >= block.len() {
            return Ok(0);
        }
        let n = buf.len().min(block.len() - offset);
        buf[..n].copy_from_slice(&block[offset..offset + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RangedFile<'_> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::End(delta) => self.len.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        let target = target.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_seeks_across_blocks_from_the_cache_after_the_first_time() {
        let data: Vec<u8> = (0..(BLOCK * 2 + 100)).map(|i| (i % 251) as u8).collect();
        let cache = Cache::new(std::env::temp_dir().join(format!("ntdc-ranged-{}", uuid::Uuid::new_v4())), u64::MAX);
        let mut fetched = 0;
        {
            let mut file = RangedFile::new(data.len() as u64, &cache, "k".into(), Box::new(|start, len| {
                fetched += 1;
                Ok(data[start as usize..(start + len) as usize].to_vec())
            }));
            file.seek(SeekFrom::End(-150)).unwrap();
            let mut tail = Vec::new();
            file.read_to_end(&mut tail).unwrap();
            assert_eq!(tail, &data[data.len() - 150..]);
            file.seek(SeekFrom::Start(10)).unwrap();
            let mut head = [0u8; 20];
            file.read_exact(&mut head).unwrap();
            assert_eq!(&head, &data[10..30]);
            assert!(file.seek(SeekFrom::Current(-100)).is_err());
        }
        assert_eq!(fetched, 3, "the last two blocks and the first");
        let mut again = RangedFile::new(data.len() as u64, &cache, "k".into(), Box::new(|_, _| panic!("fetched twice")));
        let mut all = Vec::new();
        again.seek(SeekFrom::Start(BLOCK - 5)).unwrap();
        again.by_ref().take(10).read_to_end(&mut all).unwrap();
        assert_eq!(all, &data[(BLOCK - 5) as usize..(BLOCK + 5) as usize]);
        let _ = std::fs::remove_dir_all(cache.root());
    }

    #[test]
    fn a_server_error_is_kept_for_the_caller() {
        let cache = Cache::new(std::env::temp_dir().join(format!("ntdc-ranged-{}", uuid::Uuid::new_v4())), u64::MAX);
        let mut file = RangedFile::new(1000, &cache, "e".into(), Box::new(|_, _| Err(DavError::NoRange)));
        let slot = file.error.clone();
        assert!(file.read(&mut [0; 10]).is_err());
        assert_eq!(*slot.lock().unwrap(), Some(DavError::NoRange));
    }
}
