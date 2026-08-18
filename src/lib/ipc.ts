import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

export interface CommandError {
  code: string
  message: string
  detail: string | null
}

export interface DatabaseSummary {
  displayName: string
  path: string
  mode: 'read-only'
  rowCount: number
  schemaVariant: 'current' | 'legacy'
  diagnostics: string[]
}

export interface PageCursor {
  capturedAt: number
  id: number
}

export interface InteractionSummary {
  id: number
  capturedAt: number
  capturedAtValid: boolean
  tool: string
  scheme: string
  host: string
  port: number
  method: string
  url: string
  statusCode: number
  mimeType: string
  responseLength: number
  notesPreview: string | null
  colour: string | null
  tag: string | null
  icon: string | null
  metadataValid: boolean
}

export interface InteractionPage {
  rows: InteractionSummary[]
  nextCursor: PageCursor | null
}

export interface MessagePreview {
  bytes: number[]
  totalLength: number
  truncated: boolean
}

export interface InteractionDetail {
  id: number
  request: MessagePreview
  response: MessagePreview
  notes: string | null
  metadata: string | null
}

export interface ExportResult {
  path: string
  byteCount: number
}

export function commandError(error: unknown): CommandError {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Partial<CommandError>
    if (typeof candidate.message === 'string') {
      return {
        code: typeof candidate.code === 'string' ? candidate.code : 'unknown',
        message: candidate.message,
        detail: typeof candidate.detail === 'string' ? candidate.detail : null,
      }
    }
  }
  return {
    code: 'unknown',
    message: typeof error === 'string' ? error : 'An unexpected error occurred.',
    detail: null,
  }
}

export async function chooseDatabasePath(): Promise<string | null> {
  const selected = await open({
    title: 'Open captured HTTP database',
    multiple: false,
    directory: false,
    filters: [{ name: 'SQLite database', extensions: ['sqlite', 'sqlite3', 'db'] }],
  })
  return typeof selected === 'string' ? selected : null
}

export async function openDatabase(path: string): Promise<DatabaseSummary> {
  return await invoke<DatabaseSummary>('open_database', { path })
}

export async function queryInteractions(
  cursor: PageCursor | null = null,
  pageSize = 100,
): Promise<InteractionPage> {
  return await invoke<InteractionPage>('query_interactions', { cursor, pageSize })
}

export async function getInteraction(interactionId: number): Promise<InteractionDetail> {
  return await invoke<InteractionDetail>('get_interaction', { interactionId })
}

export async function exportInteractionPart(
  interactionId: number,
  part: 'request' | 'response',
): Promise<ExportResult | null> {
  return await invoke<ExportResult | null>('export_interaction_part', { interactionId, part })
}

export async function closeDatabase(): Promise<void> {
  await invoke('close_database')
}
