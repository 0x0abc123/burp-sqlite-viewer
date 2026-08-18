import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { open } from '@tauri-apps/plugin-dialog'

export function currentDetailWindowId(): number | null {
  try {
    const match = /^detail-(\d+)$/.exec(getCurrentWebviewWindow().label)
    return match ? Number(match[1]) : null
  } catch {
    // Browser-only frontend development has no Tauri window metadata.
    return null
  }
}

export interface CommandError {
  code: string
  message: string
  detail: string | null
}

export interface DatabaseSummary {
  displayName: string
  path: string
  mode: 'read-only' | 'read-write'
  rowCount: number
  schemaVariant: 'current' | 'legacy'
  diagnostics: string[]
}

export interface PageCursor {
  offset: number
}

export type SortField =
  | 'id'
  | 'capturedAt'
  | 'tool'
  | 'scheme'
  | 'host'
  | 'port'
  | 'method'
  | 'url'
  | 'statusCode'
  | 'mimeType'
  | 'responseLength'
  | 'colour'
  | 'tag'
  | 'icon'
export type SortDirection = 'asc' | 'desc'
export type FilterField = SortField
export type FilterOperator =
  | 'contains'
  | 'equals'
  | 'startsWith'
  | 'endsWith'
  | 'isEmpty'
  | 'greaterThan'
  | 'lessThan'

export interface FilterSpec {
  field: FilterField
  operator: FilterOperator
  value: string
}

export interface QuerySpec {
  sortField: SortField
  sortDirection: SortDirection
  filters: FilterSpec[]
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
  filteredCount: number
}

export interface MessagePreview {
  bytes: number[]
  totalLength: number
  truncated: boolean
}

export interface InteractionDetail {
  id: number
  capturedAt: number
  method: string
  url: string
  statusCode: number
  mimeType: string
  request: MessagePreview
  response: MessagePreview
  notes: string | null
  metadata: string | null
}

export interface ExportResult {
  path: string
  byteCount: number
}

export interface AnnotationInput {
  interactionIds: number[]
  updateNotes: boolean
  notes: string
  updateColour: boolean
  colour: string
  updateTag: boolean
  tag: string
  updateIcon: boolean
  icon: string
}

export interface AnnotationResult {
  updatedCount: number
}

export interface ReplayInput {
  interactionId: number
  targetUrl: string
  draftBytes: number[] | null
  proxyUrl: string | null
  proxyUsername: string
  proxyPassword: string
  followRedirects: boolean
  timeoutSeconds: number
}

export interface ReplayResult {
  historyId: number
  requestBytes: number[]
  responseBytes: number[]
  statusCode: number | null
  elapsedMillis: number
  error: string | null
  normalisedByHttpClient: boolean
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

export async function enableAnnotations(): Promise<DatabaseSummary> {
  return await invoke<DatabaseSummary>('enable_annotations')
}

export async function annotateInteractions(annotation: AnnotationInput): Promise<AnnotationResult> {
  return await invoke<AnnotationResult>('annotate_interactions', { annotation })
}

export async function replayRequest(replay: ReplayInput): Promise<ReplayResult> {
  return await invoke<ReplayResult>('replay_request', { replay })
}

export async function queryInteractions(
  query: QuerySpec,
  cursor: PageCursor | null = null,
  pageSize = 100,
): Promise<InteractionPage> {
  return await invoke<InteractionPage>('query_interactions', { query, cursor, pageSize })
}

export async function getInteraction(interactionId: number): Promise<InteractionDetail> {
  return await invoke<InteractionDetail>('get_interaction', { interactionId })
}

export async function openDetailWindow(interactionId: number): Promise<void> {
  await invoke('open_detail_window', { interactionId })
}

export async function exportInteractionPart(
  interactionId: number,
  part: 'request' | 'response',
): Promise<ExportResult | null> {
  return await invoke<ExportResult | null>('export_interaction_part', { interactionId, part })
}

export async function saveMessageSelection(
  interactionId: number,
  part: 'request' | 'response',
  text: string,
): Promise<ExportResult | null> {
  return await invoke<ExportResult | null>('save_message_selection', { interactionId, part, text })
}

export async function closeDatabase(): Promise<void> {
  await invoke('close_database')
}
