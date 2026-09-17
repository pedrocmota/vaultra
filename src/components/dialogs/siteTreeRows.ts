import type {SiteNode, SiteTree} from '@/lib/api'

export type TreeRow =
  | {kind: 'folder', id: string, parentId: string | null, expandable: boolean}
  | {kind: 'site', id: string, parentId: string | null, expandable: boolean}
  | {kind: 'bookmark', id: string, siteId: string, parentId: string | null, expandable: false}

export type TreeSelection =
  | {kind: 'folder', id: string}
  | {kind: 'site', id: string}
  | {kind: 'bookmark', siteId: string, id: string}
  | null

export function visibleRows(tree: SiteTree, expanded: Set<string>): TreeRow[] {
  const rows: TreeRow[] = []

  const walk = (nodes: SiteNode[], parentId: string | null) => {
    for (const node of nodes) {
      if (node.kind === 'folder') {
        rows.push({kind: 'folder', id: node.id, parentId, expandable: node.children.length > 0})

        if (expanded.has(node.id)) {
          walk(node.children, node.id)
        }

        continue
      }

      const siteId = node.site.id
      rows.push({kind: 'site', id: siteId, parentId, expandable: node.bookmarks.length > 0})

      if (expanded.has(siteId)) {
        for (const bookmark of node.bookmarks) {
          rows.push(
            {kind: 'bookmark', id: bookmark.id, siteId, parentId: siteId, expandable: false}
          )
        }
      }
    }
  }

  walk(tree.root, null)

  return rows
}

export function rowIndexOf(rows: TreeRow[], selection: TreeSelection): number {
  if (!selection) {
    return -1
  }

  return rows.findIndex((row) => row.kind === selection.kind && row.id === selection.id)
}

export function selectionOf(row: TreeRow): TreeSelection {
  if (row.kind === 'bookmark') {
    return {kind: 'bookmark', siteId: row.siteId, id: row.id}
  }

  return {kind: row.kind, id: row.id}
}

export function folderPathTo(tree: SiteTree, siteId: string): string[] | null {
  const walk = (nodes: SiteNode[], path: string[]): string[] | null => {
    for (const node of nodes) {
      if (node.kind === 'site') {
        if (node.site.id === siteId) {
          return path
        }

        continue
      }

      const found = walk(node.children, [...path, node.id])

      if (found) {
        return found
      }
    }

    return null
  }

  return walk(tree.root, [])
}