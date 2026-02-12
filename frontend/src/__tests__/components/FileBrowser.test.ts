import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import FileBrowser from '../../components/FileBrowser.vue'

const mockEntries = [
  {
    name: 'components', is_dir: true, size: 0, modified: null,
    children: [{ name: 'Button.vue', is_dir: false, size: 500, modified: null }],
  },
  { name: 'index.ts', is_dir: false, size: 1200, modified: null },
  { name: 'utils.ts', is_dir: false, size: 3400, modified: null },
]

const mockListFiles = vi.fn().mockResolvedValue({ path: '/', entries: mockEntries })
const mockDownloadFile = vi.fn().mockResolvedValue(undefined)
const mockDownloadBatch = vi.fn().mockResolvedValue(undefined)

vi.mock('../../api/conversations', () => ({
  listFiles: (...args: unknown[]) => mockListFiles(...args),
  downloadFile: (...args: unknown[]) => mockDownloadFile(...args),
  downloadBatch: (...args: unknown[]) => mockDownloadBatch(...args),
}))

async function mountBrowser() {
  const wrapper = mount(FileBrowser, {
    props: { conversationId: 'conv-1' },
    global: { plugins: [ElementPlus] },
  })
  await flushPromises()
  return wrapper
}

function getNodeLabels(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('.fb-name').map(el => el.text())
}

function getCheckboxes(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('.el-tree .el-checkbox')
}

describe('FileBrowser', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockListFiles.mockResolvedValue({ path: '/', entries: mockEntries })
  })

  it('renders file tree with all nodes expanded', async () => {
    const wrapper = await mountBrowser()
    const labels = getNodeLabels(wrapper)
    // default-expand-all: all 4 nodes visible
    expect(labels).toEqual(['components', 'Button.vue', 'index.ts', 'utils.ts'])
  })

  it('calls listFiles with recursive=true', async () => {
    await mountBrowser()
    expect(mockListFiles).toHaveBeenCalledWith('conv-1', '/', true)
  })

  it('checkbox toggles selection and shows selection bar', async () => {
    const wrapper = await mountBrowser()
    expect(wrapper.find('.fb-selection-bar').exists()).toBe(false)

    // Check "index.ts" (3rd checkbox: components, Button.vue, index.ts)
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[2].find('input').setValue(true)
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
    expect(wrapper.find('.fb-selection-count').text()).toBe('4 selected')

    // Deselect all
    await selectAll.find('input').setValue(false)
    await flushPromises()
    expect(wrapper.find('.fb-selection-bar').exists()).toBe(false)
  })

  it('parent checkbox cascades to children', async () => {
    const wrapper = await mountBrowser()
    // Check "components" dir (first checkbox)
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[0].find('input').setValue(true)
    await flushPromises()

    // Both components and Button.vue should be checked
    const checked = wrapper.findAll('.el-tree .is-checked')
    expect(checked.length).toBeGreaterThanOrEqual(2)
  })

  it('download button calls downloadFile', async () => {
    const wrapper = await mountBrowser()
    const downloadBtns = wrapper.findAll('.fb-download')
    // Click download on index.ts (3rd node: components, Button.vue, index.ts)
    await downloadBtns[2].trigger('click')
    await flushPromises()
    expect(mockDownloadFile).toHaveBeenCalledWith('conv-1', '/index.ts')
  })

  it('batch download with single selection uses downloadFile', async () => {
    const wrapper = await mountBrowser()
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[2].find('input').setValue(true) // index.ts
    await flushPromises()

    await wrapper.find('.fb-selection-bar button').trigger('click')
    await flushPromises()
    expect(mockDownloadFile).toHaveBeenCalledWith('conv-1', '/index.ts')
    expect(mockDownloadBatch).not.toHaveBeenCalled()
  })

  it('batch download with multiple selections uses downloadBatch', async () => {
    const wrapper = await mountBrowser()
    const checkboxes = getCheckboxes(wrapper)
    await checkboxes[2].find('input').setValue(true) // index.ts
    await checkboxes[3].find('input').setValue(true) // utils.ts
    await flushPromises()

    await wrapper.find('.fb-selection-bar button').trigger('click')
    await flushPromises()
    expect(mockDownloadBatch).toHaveBeenCalledWith(
      'conv-1',
      expect.arrayContaining(['/index.ts', '/utils.ts'])
    )
  })

  it('shows error state on load failure', async () => {
    mockListFiles.mockRejectedValueOnce(new Error('fail'))
    const wrapper = await mountBrowser()
    expect(wrapper.find('.fb-error').exists()).toBe(true)
    expect(wrapper.find('.fb-error').text()).toBe('Failed to load files')
  })
})
