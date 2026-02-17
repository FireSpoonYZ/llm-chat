import { describe, it, expect } from 'vitest'
import { fileViewUrl, sharedFileViewUrl } from '../../utils/fileUrl'

describe('fileUrl helpers', () => {
  it('builds authenticated file view URL without token query', () => {
    const url = fileViewUrl('conv-1', '/images/a b.png')
    expect(url).toContain('/api/conversations/conv-1/files/view?path=')
    expect(url).toContain(encodeURIComponent('/images/a b.png'))
    expect(url).not.toContain('&token=')
    expect(url).not.toContain('token=')
  })

  it('builds shared file view URL', () => {
    const url = sharedFileViewUrl('share-abc', '/hello.txt')
    expect(url).toBe(`/api/shared/share-abc/files/view?path=${encodeURIComponent('/hello.txt')}`)
  })
})
