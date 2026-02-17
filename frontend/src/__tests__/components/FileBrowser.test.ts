import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import FileBrowser from '../../components/FileBrowser.vue'

const mockRootEntries = [
  { name: 'components', is_dir: true, size: 0, modified: null },
  { name: 'index.ts', is_dir: false, size: 1200, modified: null },
  { name: 'utils.ts', is_dir: false, size: 3400, modified: null },
]

const mockListFiles = vi.fn()
const mockDownloadFile = vi.fn().mockResolvedValue(undefined)
const mockDownloadBatch = vi.fn().mockResolvedValue(undefined)
const mockUploadFiles = vi.fn().mockResolvedValue({ uploaded: [{ name: 'test.txt', size: 100, path: '/test.txt' }] })

vi.mock('../../api/conversations', () => ({
  listFiles: (...args: unknown[]) => mockListFiles(...args),
  downloadFile: (...args: unknown[]) => mockDownloadFile(...args),
  downloadBatch: (...args: unknown[]) => mockDownloadBatch(...args),
  uploadFiles: (...args: unknown[]) => mockUploadFiles(...args),
}))

async function mountBrowser() {
  const wrapper = mount(FileBrowser, {
    props: { conversationId: 'conv-1' },
    global: { plugins: [ElementPlus] },
  })
  await flushPromises()
  return wrapper
}

async function mountBrowserWithRootEntries(entries: Array<{ name: string; is_dir: boolean; size: number; modified: string | null }>) {
  mockListFiles.mockImplementation((_id: string, path = '/', _recursive = false) => {
    if (!path || path === '/') return Promise.resolve({ path: '/', entries })
    return Promise.resolve({ path, entries: [] })
  })
  return mountBrowser()
}

function getNodeLabels(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('.fb-name').map(el => el.text())
}

function getCheckboxes(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('.el-tree .el-checkbox')
}

function getTreeLoadFn(wrapper: ReturnType<typeof mount>) {
  return wrapper.findComponent({ name: 'ElTree' }).props('load') as (
    node: unknown,
    resolve: (children: unknown[]) => void,
  ) => Promise<void> | void
}

describe('FileBrowser', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockListFiles.mockImplementation((_id: string, path = '/', _recursive = false) => {
      if (!path || path === '/') {
        return Promise.resolve({ path: '/', entries: mockRootEntries })
      }
      if (path === '/components') {
        return Promise.resolve({
          path,
          entries: [{ name: 'Button.vue', is_dir: false, size: 500, modified: null }],
        })
      }
      return Promise.resolve({ path, entries: [] })
    })
  })

  it('renders only root-level nodes initially', async () => {
    const wrapper = await mountBrowser()
    const labels = getNodeLabels(wrapper)
    expect(labels).toEqual(['components', 'index.ts', 'utils.ts'])
  })

  it('calls listFiles with recursive=false for root', async () => {
    await mountBrowser()
    expect(mockListFiles).toHaveBeenCalledWith('conv-1', '/', false)
  })

  it('loads folder children lazily through tree load callback', async () => {
    const wrapper = await mountBrowser()
    mockListFiles.mockClear()

    const load = getTreeLoadFn(wrapper)
    const resolve = vi.fn()
    await load(
      {
        level: 1,
        data: { name: 'components', path: '/components', is_dir: true, size: 0, modified: null, leaf: false },
      },
      resolve,
    )

    expect(mockListFiles).toHaveBeenCalledWith('conv-1', '/components', false)
    expect(resolve).toHaveBeenCalledWith([
      expect.objectContaining({ name: 'Button.vue', path: '/components/Button.vue' }),
    ])
  })

  it('reuses cached folder children on repeated loads', async () => {
    const wrapper = await mountBrowser()
    mockListFiles.mockClear()

    const load = getTreeLoadFn(wrapper)
    const node = {
      level: 1,
      data: { name: 'components', path: '/components', is_dir: true, size: 0, modified: null, leaf: false },
    }

    await load(node, vi.fn())
    await load(node, vi.fn())

    expect(mockListFiles).toHaveBeenCalledTimes(1)
    expect(mockListFiles).toHaveBeenCalledWith('conv-1', '/components', false)
  })

  it('checkbox toggles selection and shows selection bar', async () => {
    const wrapper = await mountBrowser()
    expect(wrapper.find('.fb-selection-bar').exists()).toBe(false)

    // Check "index.ts" (2nd checkbox: components, index.ts)
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[1].find('input').setValue(true)
    await flushPromises()

    expect(wrapper.find('.fb-selection-bar').exists()).toBe(true)
    expect(wrapper.find('.fb-selection-count').text()).toContain('selected')
  })

  it('select all / deselect all', async () => {
    const wrapper = await mountBrowser()
    const selectAll = wrapper.find('[data-testid="select-all"]')

    // Select all
    await selectAll.find('input').setValue(true)
    await flushPromises()
    expect(wrapper.find('.fb-selection-count').text()).toBe('Workspace selected')

    // Deselect all
    await selectAll.find('input').setValue(false)
    await flushPromises()
    expect(wrapper.find('.fb-selection-bar').exists()).toBe(false)
  })

  it('workspace select-all downloads entire workspace root path', async () => {
    const wrapper = await mountBrowser()
    const selectAll = wrapper.find('[data-testid="select-all"]')
    await selectAll.find('input').setValue(true)
    await flushPromises()

    await wrapper.find('.fb-selection-bar button').trigger('click')
    await flushPromises()

    expect(mockDownloadFile).toHaveBeenCalledWith('conv-1', '/')
    expect(mockDownloadBatch).not.toHaveBeenCalled()
  })

  it('manual check change exits workspace select-all mode', async () => {
    const wrapper = await mountBrowser()
    const selectAll = wrapper.find('[data-testid="select-all"]')
    await selectAll.find('input').setValue(true)
    await flushPromises()

    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[1].find('input').setValue(false)
    await flushPromises()

    expect(wrapper.find('.fb-selection-count').text()).not.toBe('Workspace selected')
  })

  it('download button calls downloadFile', async () => {
    const wrapper = await mountBrowser()
    const downloadBtns = wrapper.findAll('.fb-download')
    // Click download on index.ts (2nd node: components, index.ts)
    await downloadBtns[1].trigger('click')
    await flushPromises()
    expect(mockDownloadFile).toHaveBeenCalledWith('conv-1', '/index.ts')
  })

  it('batch download with single selection uses downloadFile', async () => {
    const wrapper = await mountBrowser()
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[1].find('input').setValue(true) // index.ts
    await flushPromises()

    await wrapper.find('.fb-selection-bar button').trigger('click')
    await flushPromises()
    expect(mockDownloadFile).toHaveBeenCalledWith('conv-1', '/index.ts')
    expect(mockDownloadBatch).not.toHaveBeenCalled()
  })

  it('batch download with multiple selections uses downloadBatch', async () => {
    const wrapper = await mountBrowser()
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[1].find('input').setValue(true) // index.ts
    await checkboxes[2].find('input').setValue(true) // utils.ts
    await flushPromises()

    await wrapper.find('.fb-selection-bar button').trigger('click')
    await flushPromises()
    expect(mockDownloadBatch).toHaveBeenCalledWith(
      'conv-1',
      expect.arrayContaining(['/index.ts', '/utils.ts'])
    )
  })

  it('prevents batch download when selected paths exceed limit', async () => {
    const manyEntries = Array.from({ length: 101 }, (_, i) => ({
      name: `file-${i}.txt`,
      is_dir: false,
      size: 10,
      modified: null,
    }))
    const wrapper = await mountBrowserWithRootEntries(manyEntries)
    const checkboxes = getCheckboxes(wrapper)

    for (const checkbox of checkboxes) {
      await checkbox.find('input').setValue(true)
    }
    await flushPromises()

    await wrapper.find('.fb-selection-bar button').trigger('click')
    await flushPromises()

    expect(mockDownloadBatch).not.toHaveBeenCalled()
    expect(mockDownloadFile).not.toHaveBeenCalled()
  })

  it('shows error state on load failure', async () => {
    mockListFiles.mockRejectedValueOnce(new Error('fail'))
    const wrapper = await mountBrowser()
    expect(wrapper.find('.fb-error').exists()).toBe(true)
    expect(wrapper.find('.fb-error').text()).toBe('Failed to load files')
  })

  it('renders upload button', async () => {
    const wrapper = await mountBrowser()
    expect(wrapper.find('[data-testid="upload-btn"]').exists()).toBe(true)
  })

  it('calls uploadFiles and refreshes after file selection', async () => {
    const wrapper = await mountBrowser()
    mockListFiles.mockClear()

    const fileInput = wrapper.find('[data-testid="file-input"]')
    const file = new File(['hello'], 'test.txt', { type: 'text/plain' })

    // Simulate file selection
    Object.defineProperty(fileInput.element, 'files', { value: [file] })
    await fileInput.trigger('change')
    await flushPromises()

    expect(mockUploadFiles).toHaveBeenCalledWith('conv-1', [file], '', expect.any(Function))
    // Should refresh file list after upload
    expect(mockListFiles).toHaveBeenCalledWith('conv-1', '/', false)
  })
})
