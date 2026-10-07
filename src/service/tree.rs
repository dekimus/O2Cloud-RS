//! Caso de uso: árbol de carpetas (`tree`).

use std::collections::HashSet;

use crate::api::client::O2CloudClient;
use crate::api::models::{Folder, Id};
use crate::error::{Error, Result};
use crate::service::resolve_folder;

/// Nodo del árbol de carpetas.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TreeNode {
    /// Id de la carpeta.
    pub id: Id,
    /// Nombre.
    pub name: String,
    /// Auto-carpeta del sistema.
    pub magic: bool,
    /// Subcarpetas.
    pub children: Vec<TreeNode>,
}

/// Árbol de carpetas a partir de una ruta raíz.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Tree {
    /// Ruta solicitada.
    pub path: String,
    /// Raíz del árbol.
    pub root: TreeNode,
    /// `true` si se pudo construir la jerarquía (el backend devolvió `parentid`).
    pub hierarchical: bool,
    /// Número total de carpetas (incluida la raíz).
    pub folder_count: usize,
}

/// Construye el árbol de carpetas desde `remote` (por defecto la raíz).
///
/// La jerarquía se reconstruye con `parentid` cuando el backend lo devuelve.
/// Si no lo devuelve, se ofrece una lista plana (best-effort) con
/// `hierarchical = false`.
pub async fn tree(client: &O2CloudClient, remote: &str) -> Result<Tree> {
    let folders = client.folders().list().await?;
    let root = resolve_folder(remote, &folders)
        .ok_or_else(|| Error::NotFound(format!("remote folder '{remote}' not found")))?;

    let hierarchical = folders.iter().any(|folder| folder.parentid.is_some());
    let root_node = if hierarchical {
        build_node(root, &folders, &root.id, &mut HashSet::new())
    } else {
        flat_node(root, &folders)
    };
    let folder_count = count_nodes(&root_node);

    Ok(Tree {
        path: remote.to_string(),
        root: root_node,
        hierarchical,
        folder_count,
    })
}

/// Construye recursivamente un nodo a partir de sus hijos (`parentid`).
///
/// Las carpetas sin `parentid` se consideran hijas de la raíz (el backend las
/// omite a veces en el primer nivel).
fn build_node(folder: &Folder, all: &[Folder], root_id: &Id, seen: &mut HashSet<Id>) -> TreeNode {
    seen.insert(folder.id.clone());
    let mut children = Vec::new();
    for candidate in all {
        if seen.contains(&candidate.id) {
            continue;
        }
        let is_child = candidate.parentid.as_ref() == Some(&folder.id)
            || (folder.id == *root_id && candidate.parentid.is_none());
        if is_child {
            children.push(build_node(candidate, all, root_id, seen));
        }
    }
    children.sort_by(|a, b| a.name.cmp(&b.name));
    TreeNode {
        id: folder.id.clone(),
        name: folder.name.clone(),
        magic: folder.magic,
        children,
    }
}

/// Nodo raíz con todas las demás carpetas como hijas directas (fallback).
fn flat_node(root: &Folder, all: &[Folder]) -> TreeNode {
    let mut children: Vec<TreeNode> = all
        .iter()
        .filter(|folder| folder.id != root.id)
        .map(|folder| TreeNode {
            id: folder.id.clone(),
            name: folder.name.clone(),
            magic: folder.magic,
            children: Vec::new(),
        })
        .collect();
    children.sort_by(|a, b| a.name.cmp(&b.name));
    TreeNode {
        id: root.id.clone(),
        name: root.name.clone(),
        magic: root.magic,
        children,
    }
}

/// Cuenta los nodos del árbol (incluida la raíz).
fn count_nodes(node: &TreeNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: u64, name: &str, parent: Option<u64>) -> Folder {
        Folder {
            id: Id::from(id),
            name: name.to_string(),
            status: None,
            magic: false,
            offline: false,
            creationdate: None,
            date: None,
            parentid: parent.map(Id::from),
        }
    }

    #[test]
    fn builds_hierarchy_from_parentid() {
        let folders = vec![
            folder(1, "/", None),
            folder(2, "fotos", Some(1)),
            folder(3, "2024", Some(2)),
            folder(4, "documentos", Some(1)),
        ];
        let node = build_node(&folders[0], &folders, &folders[0].id, &mut HashSet::new());
        assert_eq!(node.name, "/");
        assert_eq!(node.children.len(), 2);
        // Orden alfabético: documentos, fotos.
        assert_eq!(node.children[0].name, "documentos");
        assert_eq!(node.children[1].name, "fotos");
        assert_eq!(node.children[1].children[0].name, "2024");
        assert_eq!(count_nodes(&node), 4);
    }

    #[test]
    fn treats_parentless_folders_as_root_children() {
        // Carpeta de primer nivel sin `parentid` (el backend a veces lo omite).
        let folders = vec![
            folder(1, "/", None),
            folder(2, "sin_parent", None),
            folder(3, "con_parent", Some(1)),
        ];
        let node = build_node(&folders[0], &folders, &folders[0].id, &mut HashSet::new());
        assert_eq!(node.children.len(), 2);
        let names: Vec<_> = node.children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"sin_parent"));
        assert!(names.contains(&"con_parent"));
    }

    #[test]
    fn flat_fallback_without_parentid() {
        let folders = vec![
            folder(1, "/", None),
            folder(2, "a", None),
            folder(3, "b", None),
        ];
        let node = flat_node(&folders[0], &folders);
        assert_eq!(node.children.len(), 2);
        assert_eq!(count_nodes(&node), 3);
    }
}
