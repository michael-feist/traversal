// TODO(mfeist)
//
// - Make links with multiple targets work better in vscode. Defaults to first.

use std::{
    error::Error,
    path::{Path, PathBuf},
};

use tracing::debug_span;

use tracing_subscriber::EnvFilter;
use traversal_core::{TagRegistry, aggregate_tags, find_tags};

use lsp_types::{
    ChangeNotifications, DidChangeWatchedFilesNotification, DidChangeWatchedFilesParams,
    DidChangeWatchedFilesRegistrationOptions, DidChangeWorkspaceFoldersNotification,
    DidChangeWorkspaceFoldersParams, DocumentLink, DocumentLinkOptions, DocumentLinkParams,
    DocumentLinkRequest, FileSystemWatcher, GlobPattern, InitializeParams, LspNotificationMethod,
    LspRequestMethod, Notification, Position, Range, Registration, RegistrationParams, Request,
    ServerCapabilities, TextDocumentSync, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
    WorkspaceFolders, WorkspaceFoldersServerCapabilities, WorkspaceOptions,
};

#[allow(
    clippy::print_stderr,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use anyhow::Result;
use lsp_server::{
    Connection, Message, Request as ServerRequest, RequestId, Response, ResponseKind,
};

/// Persistent state of the LSP server. Used to resolve requests from the client.
struct TraversalLspState {
    workspace_folders: Vec<PathBuf>,
    tags: TagRegistry,
}

impl Default for TraversalLspState {
    fn default() -> Self {
        TraversalLspState {
            workspace_folders: Vec::new(),
            tags: TagRegistry::new(),
        }
    }
}

fn _print_tags(tags: &TagRegistry) {
    for target in tags.target_tags.tags.iter() {
        log::info!(
            "[TGT] [{}]: {}:{}",
            target.id,
            target.file_path.to_str().unwrap(),
            target.line_number
        );
    }
    for link in tags.link_tags.tags.iter() {
        log::info!(
            "[LNK] [{}]: {}:{}",
            link.id,
            link.file_path.to_str().unwrap(),
            link.line_number
        );
    }
}

#[allow(clippy::print_stderr)]
fn main() -> std::result::Result<(), Box<dyn Error + Sync + Send>> {
    // Init tracing
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    log::info!("Starting traversal-lsp");

    // Init LSP server connection
    let (connection, io_thread) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSync::Kind(TextDocumentSyncKind::Full)),
        document_link_provider: Some(DocumentLinkOptions::new(
            Some(false),
            WorkDoneProgressOptions::new(Some(false)),
        )),
        workspace: Some(WorkspaceOptions::new(
            Some(WorkspaceFoldersServerCapabilities::new(
                Some(true),
                Some(ChangeNotifications::Bool(true)),
            )),
            None,
            None,
        )),
        ..Default::default()
    };
    let (init_id, init_params) = connection.initialize_start()?;
    let init_value = serde_json::json!({
        "capabilities": caps,
    });
    connection.initialize_finish(init_id, init_value)?;

    // Set up LSP server state
    let mut traversal_lsp_state = TraversalLspState::default();
    let init: InitializeParams = serde_json::from_value(init_params)?;

    // Ensure client supports required LSP capabilities
    init.capabilities
        .workspace
        .expect("Client doesn't support workspaces")
        .did_change_watched_files
        .expect("Client does not support 'DidChangeWatchedFiles'")
        .dynamic_registration
        .expect("Client does not support dyanmic registration");

    // Set up file watcher
    let options = DidChangeWatchedFilesRegistrationOptions::new(vec![FileSystemWatcher::new(
        GlobPattern::Pattern("**/*".to_string()),
        None,
    )]);
    let watcher_registration = Registration::new(
        "traversal-watcher".into(),
        "workspace/didChangeWatchedFiles".into(),
        Some(serde_json::to_value(options).unwrap()),
    );
    let registration_params = RegistrationParams::new(vec![watcher_registration]);
    connection
        .sender
        .send(Message::Request(lsp_server::Request::new(
            RequestId::from(1),
            "client/registerCapability".into(),
            registration_params,
        )))
        .expect("Failed to send watcher registration");

    // Extract initial workspace folders from init
    if let Some(workspace_folders) = init.workspace_folders_initialize_params.workspace_folders
        && let WorkspaceFolders::WorkspaceFolderList(workspace_folders_list) = workspace_folders
    {
        for folder in workspace_folders_list {
            assert_eq!(folder.uri.scheme(), "file");
            log::info!("Adding workspace folder: {}", folder.uri.path());
            traversal_lsp_state
                .workspace_folders
                .push(PathBuf::from(folder.uri.path()));
        }
    }

    // Run our first tag find
    {
        let _tracing_span = debug_span!("find_tags_init").entered();
        traversal_lsp_state.tags =
            aggregate_tags(find_tags(&traversal_lsp_state.workspace_folders));
    }

    // Core server loop
    server_message_loop(&mut traversal_lsp_state, connection)?;
    io_thread.join()?;

    log::info!("Shutting down...");
    Ok(())
}

fn server_message_loop(
    traversal_lsp_state: &mut TraversalLspState,
    connection: Connection,
) -> std::result::Result<(), Box<dyn Error + Sync + Send>> {
    // Loop on incoming messages until we receive a shutdown
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                log::debug!("Received request '{}'", req.method);
                if let Err(err) = handle_request(&connection, &req, traversal_lsp_state) {
                    log::error!("Request {} failed: {err}", req.method);
                }
            }
            Message::Notification(note) => {
                log::debug!("Received notification '{}'", note.method);
                if let Err(err) = handle_notification(&note, traversal_lsp_state) {
                    log::error!("Notification {} failed: {err}", note.method);
                }
            }
            Message::Response(resp) => log::debug!("Received response '{resp:?}'"),
        }
    }
    Ok(())
}

// =====================
// Notifications
// =====================

fn handle_notification(
    note: &lsp_server::Notification,
    traversal_lsp_state: &mut TraversalLspState,
) -> Result<()> {
    let method: LspNotificationMethod<'_> = note.method.as_str().into();
    match method {
        DidChangeWorkspaceFoldersNotification::METHOD => {
            handle_did_change_workspace_folders_notification(note)?
        }
        DidChangeWatchedFilesNotification::METHOD => {
            handle_did_change_watched_files_notification(note, traversal_lsp_state)?
        }
        _ => {
            log::debug!("Received unhandled notification type '{}'", method);
        }
    }
    Ok(())
}

fn handle_did_change_workspace_folders_notification(note: &lsp_server::Notification) -> Result<()> {
    let params: DidChangeWorkspaceFoldersParams = serde_json::from_value(note.params.clone())?;
    for added in &params.event.added {
        log::info!("Added workspace folder '{}': {}", added.name, added.uri);
    }
    for removed in &params.event.removed {
        log::info!(
            "Removed workspace folder '{}': {}",
            removed.name,
            removed.uri
        );
    }
    Ok(())
}

fn handle_did_change_watched_files_notification(
    note: &lsp_server::Notification,
    traversal_lsp_state: &mut TraversalLspState,
) -> Result<()> {
    let _p: DidChangeWatchedFilesParams = serde_json::from_value(note.params.clone())?;
    log::info!("Received DidChangeWatchedFiles");
    {
        let _tracing_span = debug_span!("find_tags").entered();
        traversal_lsp_state.tags =
            aggregate_tags(find_tags(&traversal_lsp_state.workspace_folders));
    }
    Ok(())
}

// ==============
// Requests
// ==============

fn handle_request(
    conn: &Connection,
    req: &ServerRequest,
    traversal_lsp_state: &TraversalLspState,
) -> Result<()> {
    let parsed: LspRequestMethod<'_> = req.method.as_str().into();
    match parsed {
        DocumentLinkRequest::METHOD => {
            handle_document_link_request(conn, req, traversal_lsp_state)?
        }
        _ => send_err(
            conn,
            req.id.clone(),
            lsp_server::ErrorCode::MethodNotFound,
            "unhandled method",
        )?,
    }
    Ok(())
}

fn handle_document_link_request(
    conn: &Connection,
    req: &ServerRequest,
    traversal_lsp_state: &TraversalLspState,
) -> Result<()> {
    let document_link_req: DocumentLinkParams = serde_json::from_value(req.params.clone())?;

    // We only accept files (for now at least)
    assert_eq!(document_link_req.text_document.uri.scheme(), "file");
    let document_path = Path::new(document_link_req.text_document.uri.path());

    let link_tags_in_file_opt = traversal_lsp_state
        .tags
        .link_tags
        .tag_indices_by_file
        .get(document_path);

    /// - Creates an empty document link list.
    /// - Loops over all of the link_tags and finds every matching target_tag.
    /// - Loops over all of the matching target tags.
    /// - Create a DocumentLink based on the link_tag and matching target_tag.
    /// - Pushes the newly created DocumentLink onto the list.
    fn create_document_links_list(
        traversal_lsp_state: &TraversalLspState,
        link_tag_idxs: &Vec<usize>,
    ) -> Vec<DocumentLink> {
        // Create an empty document list
        let mut document_links = Vec::<DocumentLink>::new();

        // Loop over all the link tags
        for link_tag_idx in link_tag_idxs {
            let link_tag = &traversal_lsp_state.tags.link_tags.tags[*link_tag_idx];
            if let Some(target_tag_idxs) = traversal_lsp_state
                .tags
                .target_tags
                .tag_indices_by_id
                .get(link_tag.id.as_str())
            {
                // Loop over all of the matching target tags
                for target_tag_idx in target_tag_idxs {
                    let target_tag = &traversal_lsp_state.tags.target_tags.tags[*target_tag_idx];
                    log::debug!("Resolving link tag: {:?}", link_tag);
                    log::debug!("Resolved to target tag: {:?}", target_tag);

                    // Create a DocumentLink based on the link_tag and matching target_tag
                    let link_tag_line_number_zero_based: u32 =
                        link_tag.line_number.saturating_sub(1).try_into().unwrap();
                    let target_tag_column_start_one_based =
                        target_tag.range.start.saturating_add(1);
                    let document_link = DocumentLink::new(
                        Range::new(
                            Position::new(
                                link_tag_line_number_zero_based,
                                link_tag.range.start.try_into().unwrap(),
                            ),
                            Position::new(
                                link_tag_line_number_zero_based,
                                link_tag.range.end.try_into().unwrap(),
                            ),
                        ),
                        Some(
                            Uri::parse(
                                format!(
                                    "file://{}#L{},{}",
                                    target_tag.file_path.to_str().unwrap(),
                                    target_tag.line_number,
                                    target_tag_column_start_one_based
                                )
                                .as_str(),
                            )
                            .expect("Invalid URI parse"),
                        ),
                        None,
                        None,
                    );

                    // Push the new DocumentLink onto the list
                    document_links.push(document_link);
                }
            }
        }

        return document_links;
    }

    match link_tags_in_file_opt {
        Some(link_tag_idxs) => {
            let document_links = create_document_links_list(traversal_lsp_state, link_tag_idxs);
            send_ok(conn, req.id.clone(), &document_links)?
        }
        None => send_ok(conn, req.id.clone(), &Vec::<DocumentLink>::new())?,
    }
    Ok(())
}

// =====================================================================
// Helpers
// =====================================================================

fn send_ok<T: serde::Serialize>(conn: &Connection, id: RequestId, result: &T) -> Result<()> {
    let resp = Response {
        id,
        response_kind: ResponseKind::Ok {
            result: serde_json::to_value(result)?,
        },
    };
    conn.sender.send(Message::Response(resp))?;
    Ok(())
}

fn send_err(
    conn: &Connection,
    id: RequestId,
    code: lsp_server::ErrorCode,
    msg: &str,
) -> Result<()> {
    let resp = Response {
        id,
        response_kind: ResponseKind::Err {
            error: lsp_server::ResponseError {
                code: code as i32,
                message: msg.into(),
                data: None,
            },
        },
    };
    conn.sender.send(Message::Response(resp))?;
    Ok(())
}
