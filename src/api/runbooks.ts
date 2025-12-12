import { RemoteRunbook } from "@/state/models";
import Runbook from "@/state/runbooks/runbook";
import { get, post, put, del } from "./api";
import Snapshot from "@/state/runbooks/snapshot";

export async function allRunbookIds(): Promise<string[]> {
  const { runbooks } = await get<{ runbooks: string[] }>("/runbooks?id_only=true");
  return runbooks;
}

export async function getRunbookID(id: string): Promise<RemoteRunbook> {
  const { runbook } = await get<{ runbook: RemoteRunbook }>(
    `/runbooks/${id}?include=owner,snapshots,collaborations`,
  );
  return runbook;
}

export async function getRunbookYdoc(id: string): Promise<Uint8Array | null> {
  let url = `/runbooks/${id}/yjs`;

  const ydoc = await get<ArrayBuffer>(url, { bodyType: "bytes" });

  return new Uint8Array(ydoc);
}

export function createRunbook(runbook: Runbook, slug: string, visibility: string) {
  const body = {
    runbook: {
      id: runbook.id,
      name: runbook.name,
      slug: slug,
      version: 0,
      created: runbook.created,
      visibility: visibility,
      workspace_id: runbook.workspaceId,
    },
  };

  return post("/runbooks", body);
}

export function updateRunbook(runbook: Runbook, slug: string, visibility: string) {
  const body = {
    runbook: {
      id: runbook.id,
      name: runbook.name,
      slug: slug,
      version: 0,
      created: runbook.created,
      visibility: visibility,
    },
  };

  return put(`/runbooks/${runbook.id}`, body);
}

export function updateRunbookName(id: string, name: string) {
  const body = {
    runbook: {
      name: name,
    },
  };

  return put(`/runbooks/${id}`, body);
}

export function deleteRunbook(id: string) {
  return del(`/runbooks/${id}`);
}

export interface RemoteSnapshot {
  id: string;
  tag: string;
  runbook_id: string;
  content: any[];
  created: string;
}

export async function getSnapshotById(id: string): Promise<RemoteSnapshot> {
  const { snapshot } = await get<{ snapshot: RemoteSnapshot }>(`/snapshots/${id}`);
  return snapshot;
}

export function createSnapshot(snapshot: Snapshot) {
  const args = {
    snapshot: {
      id: snapshot.id,
      tag: snapshot.tag,
      client_created: snapshot.created,
      content: snapshot.content,
    },
  };
  return post(`/runbooks/${snapshot.runbook_id}/snapshots`, args);
}

export function deleteSnapshot(id: string) {
  return del(`/snapshots/${id}`);
}

export interface RemoteCollaboration {
  id: string;
  accepted: boolean;
  runbook: {
    id: string;
    owner: string;
    slug: string;
    name: string;
  };
}

/// Resolved runbook from NWO lookup
export interface ResolvedRunbook {
  runbook: RemoteRunbook;
  snapshot: RemoteSnapshot | null;
}

/// Resolve a runbook by NWO (name-with-owner), optionally with a tag
/// NWO format: "user/slug" or with tag "user/slug:tag"
export async function resolveRunbookByNwo(
  nwo: string,
  tag?: string,
): Promise<ResolvedRunbook> {
  let url = `/resolve/runbook?nwo=${encodeURIComponent(nwo)}`;
  if (tag) {
    url += `&tag=${encodeURIComponent(tag)}`;
  }
  return get<ResolvedRunbook>(url);
}
