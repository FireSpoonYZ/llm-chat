/**
 * Build a file-view URL with the access token in the query string.
 * Needed because `<img>`/`<video>`/`<audio>` src requests cannot carry
 * an Authorization header.
 */
export function fileViewUrl(conversationId: string, path: string): string {
  const token = localStorage.getItem('access_token')
  const base = `/api/conversations/${conversationId}/files/view?path=${encodeURIComponent(path)}`
  return token ? `${base}&token=${encodeURIComponent(token)}` : base
}

/**
 * Build a file-view URL for shared (public) conversations.
 * No auth token needed.
 */
export function sharedFileViewUrl(shareToken: string, path: string): string {
  return `/api/shared/${shareToken}/files/view?path=${encodeURIComponent(path)}`
}
