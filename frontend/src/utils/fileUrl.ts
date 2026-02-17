/**
 * Build an authenticated file-view URL.
 * Session auth is handled by HttpOnly cookies.
 */
export function fileViewUrl(conversationId: string, path: string): string {
  return `/api/conversations/${conversationId}/files/view?path=${encodeURIComponent(path)}`
}

/**
 * Build a file-view URL for shared (public) conversations.
 * No auth token needed.
 */
export function sharedFileViewUrl(shareToken: string, path: string): string {
  return `/api/shared/${shareToken}/files/view?path=${encodeURIComponent(path)}`
}
