use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use git2::{
    Cred, CredentialType, FetchOptions, ObjectType, PushOptions, RemoteCallbacks, Repository,
    Signature as GitSignature,
};
use reposync_core::{
    Blob, CommitId, FileEntry, FileMode, RepoPath, RepositoryMetadata, RepositorySnapshot,
    Signature,
};

use crate::error::Error;

/// Identity and message used when creating a commit.
#[derive(Debug, Clone)]
pub struct CommitSpec {
    /// Commit message body.
    pub message: String,
    /// Author signature; falls back to the repository/user config.
    pub author: Option<Signature>,
    /// Committer signature; falls back to the repository/user config.
    pub committer: Option<Signature>,
}

impl CommitSpec {
    /// Build a spec with only a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            author: None,
            committer: None,
        }
    }
}

/// A clone of a repository plus the handle used to talk to git.
pub struct GitRepo {
    repo: Repository,
    path: PathBuf,
}

impl std::fmt::Debug for GitRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitRepo").field("path", &self.path).finish_non_exhaustive()
    }
}

impl GitRepo {
    /// Open an existing repository at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` is not a git repository.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let repo = Repository::open(&path)?;
        Ok(Self { repo, path })
    }

    /// Initialize a new repository at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or initialized.
    pub fn init(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let repo = Repository::init(&path)?;
        Ok(Self { repo, path })
    }

    /// Initialize a new bare repository at `path` (no working tree).
    ///
    /// Bare repositories are the natural target for `push` and are used as
    /// destinations by the CLI and the bidirectional sync work.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or initialized.
    pub fn init_bare(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let repo = Repository::init_bare(&path)?;
        Ok(Self { repo, path })
    }

    /// Clone a remote `url` into a new directory at `into`.
    ///
    /// # Errors
    ///
    /// Returns an error if the clone fails (network, authentication, or an
    /// invalid URL).
    pub fn clone(url: &str, into: impl Into<PathBuf>) -> Result<Self, Error> {
        let into = into.into();
        let repo = Repository::clone(url, &into)?;
        Ok(Self { repo, path: into })
    }

    /// Open the repository at `into`, cloning it from `url` first if missing.
    ///
    /// # Errors
    ///
    /// Returns an error if the path exists but is not a repository, or if the
    /// initial clone fails.
    pub fn open_or_clone(url: &str, into: impl Into<PathBuf>) -> Result<Self, Error> {
        let into = into.into();
        if Repository::open(&into).is_ok() {
            Self::open(into)
        } else {
            Self::clone(url, into)
        }
    }

    /// The path the repository was opened or cloned at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The working directory, if the repository has one.
    #[must_use]
    pub fn workdir(&self) -> Option<&Path> {
        self.repo.workdir()
    }

    /// Set the identity used for commits, written into the repository config.
    ///
    /// # Errors
    ///
    /// Returns an error if the repository config cannot be written.
    pub fn set_identity(&self, name: &str, email: &str) -> Result<(), Error> {
        let mut config = self.repo.config()?;
        config.set_str("user.name", name)?;
        config.set_str("user.email", email)?;
        Ok(())
    }

    /// The id of the commit HEAD points at, if HEAD exists yet.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD exists but cannot be peeled to a commit.
    pub fn head_commit_id(&self) -> Result<Option<CommitId>, Error> {
        match self.repo.head() {
            Ok(head) => {
                let id = head.peel_to_commit()?.id();
                Ok(Some(commit_id(id)?))
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
            Err(e) => Err(Error::Git(e)),
        }
    }

    /// Snapshot of the tree at HEAD.
    ///
    /// Returns an empty snapshot (no metadata) when the repository has no
    /// commits yet.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD cannot be resolved to a commit or its tree
    /// cannot be read.
    pub fn head_snapshot(&self) -> Result<RepositorySnapshot, Error> {
        if self.head_commit_id()?.is_none() {
            return Ok(RepositorySnapshot::new());
        }
        self.snapshot_at_ref("HEAD")
    }

    /// Snapshot of the tree a rev-spec points at (branch, tag, or commit id).
    ///
    /// # Errors
    ///
    /// Returns an error if `reference` does not resolve to a commit.
    pub fn snapshot_at_ref(&self, reference: &str) -> Result<RepositorySnapshot, Error> {
        let commit = self.repo.revparse_single(reference)?.peel_to_commit()?;
        let tree = commit.tree()?;
        let mut files = BTreeMap::new();
        read_tree_level(&self.repo, &tree, "", &mut files)?;
        let snapshot = RepositorySnapshot {
            files,
            metadata: RepositoryMetadata {
                head: Some(commit_id(commit.id())?),
                head_message: commit.message().map(ToOwned::to_owned),
                custom: BTreeMap::new(),
            },
        };
        Ok(snapshot)
    }

    /// Commit ids in history, newest first, ending at the current HEAD.
    ///
    /// Returns an empty vector when the repository has no commits yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the revision walk fails.
    pub fn history(&self, limit: Option<usize>) -> Result<Vec<CommitId>, Error> {
        let head = match self.repo.head() {
            Ok(head) => head,
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(Vec::new()),
            Err(e) => return Err(Error::Git(e)),
        };
        let mut walk = self.repo.revwalk()?;
        walk.push(head.peel_to_commit()?.id())?;
        let mut ids = Vec::new();
        for entry in walk {
            if let Some(limit) = limit {
                if ids.len() >= limit {
                    break;
                }
            }
            ids.push(commit_id(entry?)?);
        }
        Ok(ids)
    }

    /// Write `snapshot` into git as a new commit on the current branch,
    /// returning the new commit id.
    ///
    /// The snapshot's file content is stored as blobs and the tree structure
    /// is derived from the file paths. Existing files not present in the
    /// snapshot are omitted.
    ///
    /// # Errors
    ///
    /// Returns an error if a path conflict is found (a file and a directory at
    /// the same path), a signature cannot be resolved, or writing objects
    /// fails.
    pub fn write_commit(
        &self,
        snapshot: &RepositorySnapshot,
        spec: &CommitSpec,
    ) -> Result<CommitId, Error> {
        let parent_ids = self.parent_commits()?;
        let parent_refs: Vec<&git2::Commit> = parent_ids.iter().collect();
        self.write_commit_with_parents(snapshot, spec, &parent_refs)
    }

    /// Write `snapshot` as a new commit with an explicit set of parent commits,
    /// returning the new commit id.
    ///
    /// Unlike [`GitRepo::write_commit`], this does not derive parents from the
    /// current HEAD. It is used by history migration, where each rewritten
    /// commit must point at its already-rewritten parents rather than whatever
    /// HEAD happens to be.
    ///
    /// # Errors
    ///
    /// Returns an error if a parent id cannot be resolved, a path conflict is
    /// found, a signature cannot be resolved, or writing objects fails.
    pub fn write_commit_with_parents(
        &self,
        snapshot: &RepositorySnapshot,
        spec: &CommitSpec,
        parents: &[&git2::Commit<'_>],
    ) -> Result<CommitId, Error> {
        let (author, committer) = self.resolve_signatures(spec)?;
        let tree_id = write_snapshot_tree(&self.repo, snapshot)?;
        let tree = self.repo.find_tree(tree_id)?;
        let id = self.repo.commit(
            Some("HEAD"),
            &author,
            &committer,
            &spec.message,
            &tree,
            parents,
        )?;
        commit_id(id)
    }

    /// Write `snapshot` as a new commit with parents identified by commit id.
    ///
    /// A convenience over [`GitRepo::write_commit_with_parents`] that resolves
    /// the parent ids inside the destination repository, so callers do not need
    /// to hold git2 objects.
    ///
    /// # Errors
    ///
    /// Returns an error if a parent id does not resolve to a commit, a path
    /// conflict is found, a signature cannot be resolved, or writing objects
    /// fails.
    pub fn write_commit_with_parent_ids(
        &self,
        snapshot: &RepositorySnapshot,
        spec: &CommitSpec,
        parent_ids: &[CommitId],
    ) -> Result<CommitId, Error> {
        let mut parent_commits = Vec::with_capacity(parent_ids.len());
        for pid in parent_ids {
            let oid = git2::Oid::from_str(pid.as_str()).map_err(Error::Git)?;
            parent_commits.push(self.repo.find_commit(oid)?);
        }
        let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();
        self.write_commit_with_parents(snapshot, spec, &parent_refs)
    }

    /// Read the metadata of a commit by id: message, author, committer, and
    /// parent ids.
    ///
    /// # Errors
    ///
    /// Returns an error if `id` does not resolve to a commit.
    pub fn commit_info(&self, id: &CommitId) -> Result<reposync_core::Commit, Error> {
        let oid = git2::Oid::from_str(id.as_str()).map_err(Error::Git)?;
        let commit = self.repo.find_commit(oid)?;
        let author = signature_to_model(&commit.author());
        let committer = signature_to_model(&commit.committer());
        let parents = commit
            .parent_ids()
            .map(commit_id)
            .collect::<Result<Vec<_>, _>>()?;
        let message = commit.message().unwrap_or("").to_owned();
        Ok(reposync_core::Commit::new(
            id.clone(),
            parents,
            message,
            author,
            committer,
        ))
    }

    /// Check out the current HEAD into the working directory, overwriting
    /// conflicting files.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD does not exist or the checkout fails.
    pub fn checkout(&self) -> Result<(), Error> {
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.force();
        self.repo.checkout_head(Some(&mut builder))?;
        Ok(())
    }

    /// Fetch refs from `url` into the local repository.
    ///
    /// `refspec` follows git fetch refspec syntax, e.g.
    /// `refs/heads/*:refs/remotes/origin/*`.
    ///
    /// # Errors
    ///
    /// Returns an error if the fetch fails (network, authentication, or an
    /// invalid refspec).
    pub fn fetch(&self, url: &str, refspec: &str) -> Result<(), Error> {
        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(credentials_callbacks());
        let mut remote = self.repo.remote_anonymous(url)?;
        remote.fetch(&[refspec], Some(&mut fetch_options), None)?;
        Ok(())
    }

    /// Push the current branch to `url`.
    ///
    /// The local branch is pushed to the remote branch of the same name.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD is detached or the push fails.
    pub fn push(&self, url: &str) -> Result<(), Error> {
        let branch = self.current_branch_name()?;
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(credentials_callbacks());
        let mut remote = self.repo.remote_anonymous(url)?;
        remote.push(&[refspec], Some(&mut push_options))?;
        Ok(())
    }

    /// The name of the branch HEAD points at.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD is detached or does not exist.
    pub fn current_branch_name(&self) -> Result<String, Error> {
        let head = self.repo.head()?;
        head.shorthand()
            .map(ToOwned::to_owned)
            .ok_or(Error::DetachedHead)
    }

    /// Whether the working tree and index are clean.
    ///
    /// # Errors
    ///
    /// Returns an error if the status scan fails.
    pub fn status_is_clean(&self) -> Result<bool, Error> {
        let statuses = self.repo.statuses(None)?;
        Ok(statuses.is_empty())
    }

    /// Resolve the author/committer signatures for a commit, falling back to
    /// the repository or global git config.
    fn resolve_signatures(
        &self,
        spec: &CommitSpec,
    ) -> Result<(GitSignature<'static>, GitSignature<'static>), Error> {
        let default = self.repo.signature()?;
        let author = match &spec.author {
            Some(author) => signature_from_model(author)?,
            None => default.clone(),
        };
        let committer = match &spec.committer {
            Some(committer) => signature_from_model(committer)?,
            None => default,
        };
        Ok((author, committer))
    }

    /// The commits to use as parents for a new commit (current HEAD if any).
    fn parent_commits(&self) -> Result<Vec<git2::Commit<'_>>, Error> {
        let mut parents = Vec::new();
        match self.repo.head() {
            Ok(head) => {
                if let Ok(commit) = head.peel_to_commit() {
                    parents.push(commit);
                }
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {}
            Err(e) => return Err(Error::Git(e)),
        }
        Ok(parents)
    }
}

/// Write a snapshot's files into git object storage, returning the root tree id.
fn write_snapshot_tree(
    repo: &Repository,
    snapshot: &RepositorySnapshot,
) -> Result<git2::Oid, Error> {
    let entries: Vec<(String, &FileEntry)> = snapshot
        .files
        .iter()
        .map(|(path, entry)| (path.as_str().to_owned(), entry))
        .collect();
    write_tree_level(repo, &entries)
}

/// Write one tree level from path-prefixed entries, recursing into subtrees.
fn write_tree_level(
    repo: &Repository,
    entries: &[(String, &FileEntry)],
) -> Result<git2::Oid, Error> {
    let mut builder = repo.treebuilder(None)?;
    let mut subtrees: BTreeMap<String, Vec<(String, &FileEntry)>> = BTreeMap::new();
    let mut file_names: BTreeSet<&str> = BTreeSet::new();
    for (path, entry) in entries {
        if let Some((top, rest)) = path.split_once('/') {
            subtrees
                .entry(top.to_owned())
                .or_default()
                .push((rest.to_owned(), entry));
        } else {
            if subtrees.contains_key(path.as_str()) {
                return Err(Error::PathConflict { path: path.clone() });
            }
            let blob_id = repo.blob(entry.content.content())?;
            builder.insert(path, blob_id, i32::from(git_mode(entry.mode)))?;
            file_names.insert(path);
        }
    }
    for (name, children) in &subtrees {
        if file_names.contains(name.as_str()) {
            return Err(Error::PathConflict { path: name.clone() });
        }
        let tree_id = write_tree_level(repo, children)?;
        builder.insert(name, tree_id, i32::from(git2::FileMode::Tree))?;
    }
    Ok(builder.write()?)
}

/// Read a git tree into a flat snapshot file map, depth-first.
fn read_tree_level(
    repo: &Repository,
    tree: &git2::Tree,
    prefix: &str,
    out: &mut BTreeMap<RepoPath, FileEntry>,
) -> Result<(), Error> {
    for entry in tree {
        let name = String::from_utf8_lossy(entry.name_bytes()).into_owned();
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let repopath = RepoPath::new(&path)?;
        match entry.kind() {
            Some(ObjectType::Tree) => {
                let subtree = entry.to_object(repo)?.peel_to_tree()?;
                read_tree_level(repo, &subtree, &path, out)?;
            }
            Some(ObjectType::Blob) => {
                let blob = repo.find_blob(entry.id())?;
                let mode = FileMode::from_u32(tree_entry_mode(&entry))
                    .ok_or_else(|| unsupported_mode(&entry))?;
                out.insert(
                    repopath.clone(),
                    FileEntry::new(repopath, Blob::from_bytes(blob.content().to_vec()), mode),
                );
            }
            Some(ObjectType::Commit) => {
                let mode = FileMode::from_u32(tree_entry_mode(&entry))
                    .ok_or_else(|| unsupported_mode(&entry))?;
                let content = entry.id().to_string().into_bytes();
                out.insert(
                    repopath.clone(),
                    FileEntry::new(repopath, Blob::from_bytes(content), mode),
                );
            }
            _ => {
                return Err(Error::UnsupportedEntry {
                    name,
                    mode: tree_entry_mode(&entry),
                });
            }
        }
    }
    Ok(())
}

/// The git mode of a tree entry as a non-negative integer.
fn tree_entry_mode(entry: &git2::TreeEntry<'_>) -> u32 {
    u32::try_from(entry.filemode()).unwrap_or_default()
}

/// Build a core [`Error::UnsupportedMode`] for a tree entry.
fn unsupported_mode(entry: &git2::TreeEntry<'_>) -> reposync_core::Error {
    reposync_core::Error::UnsupportedMode(tree_entry_mode(entry))
}

/// Convert a model file mode to a git2 mode.
fn git_mode(mode: FileMode) -> git2::FileMode {
    match mode {
        FileMode::File => git2::FileMode::Blob,
        FileMode::Executable => git2::FileMode::BlobExecutable,
        FileMode::Symlink => git2::FileMode::Link,
        FileMode::Gitlink => git2::FileMode::Commit,
    }
}

/// Convert a model signature into a git2 signature.
fn signature_from_model(sig: &Signature) -> Result<GitSignature<'static>, Error> {
    let when = git2::Time::new(sig.time, sig.offset_minutes);
    Ok(GitSignature::new(&sig.name, &sig.email, &when)?)
}

/// Convert a git2 signature into a model [`Signature`].
fn signature_to_model(sig: &git2::Signature<'_>) -> Signature {
    let when = sig.when();
    Signature::new(
        sig.name().unwrap_or("").to_owned(),
        sig.email().unwrap_or("").to_owned(),
        when.seconds(),
        when.offset_minutes(),
    )
}

/// Build a model [`CommitId`] from a git object id.
fn commit_id(oid: git2::Oid) -> Result<CommitId, Error> {
    Ok(CommitId::new(oid.to_string())?)
}

/// Build the credential callbacks used by fetch and push.
fn credentials_callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username, allowed| {
        if allowed.contains(CredentialType::SSH_KEY) {
            Cred::ssh_key_from_agent(username.unwrap_or("git"))
        } else if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            Cred::default()
        } else {
            Err(git2::Error::from_str("no credential helper available"))
        }
    });
    callbacks
}
