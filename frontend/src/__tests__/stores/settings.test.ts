import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore } from '../../stores/settings'
import type { SystemPromptPreset } from '../../types'

const mockPresets: SystemPromptPreset[] = [
  {
    id: 'p1',
    name: 'Default',
    description: 'Default prompt',
    content: 'You are helpful.',
    is_default: true,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'p2',
    name: 'Coder',
    description: 'Coding assistant',
    content: 'You are a coder.',
    is_default: false,
    created_at: '2025-01-02T00:00:00Z',
    updated_at: '2025-01-02T00:00:00Z',
  },
]

vi.mock('../../api/prompts', () => ({
  listPresets: vi.fn().mockResolvedValue([]),
  createPreset: vi.fn().mockResolvedValue({}),
  updatePreset: vi.fn().mockResolvedValue({}),
  deletePreset: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../api/users', () => ({
  listProviders: vi.fn().mockResolvedValue([]),
  upsertProvider: vi.fn().mockResolvedValue({}),
  deleteProvider: vi.fn().mockResolvedValue(undefined),
  listMcpServers: vi.fn().mockResolvedValue([]),
}))

import * as presetsApi from '../../api/prompts'

describe('settings store - presets', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('should load presets from API', async () => {
    vi.mocked(presetsApi.listPresets).mockResolvedValueOnce(mockPresets)
    const store = useSettingsStore()
    await store.loadPresets()
    expect(store.presets).toHaveLength(2)
    expect(store.presets[0].name).toBe('Default')
  })

  it('should compute defaultPreset from is_default flag', async () => {
    vi.mocked(presetsApi.listPresets).mockResolvedValueOnce(mockPresets)
    const store = useSettingsStore()
    await store.loadPresets()
    expect(store.defaultPreset).toBeDefined()
    expect(store.defaultPreset!.id).toBe('p1')
    expect(store.defaultPreset!.name).toBe('Default')
  })

  it('should fall back to first preset when none is default', async () => {
    const noDefault = mockPresets.map(p => ({ ...p, is_default: false }))
    vi.mocked(presetsApi.listPresets).mockResolvedValueOnce(noDefault)
    const store = useSettingsStore()
    await store.loadPresets()
    expect(store.defaultPreset).toBeDefined()
    expect(store.defaultPreset!.id).toBe('p1')
  })

  it('should return undefined defaultPreset when no presets', () => {
    const store = useSettingsStore()
    expect(store.defaultPreset).toBeUndefined()
  })

  it('should call createPreset and reload on savePreset', async () => {
    vi.mocked(presetsApi.listPresets).mockResolvedValue(mockPresets)
    const store = useSettingsStore()
    await store.savePreset({ name: 'New', content: 'test' })
    expect(presetsApi.createPreset).toHaveBeenCalledWith({ name: 'New', content: 'test' })
    expect(presetsApi.listPresets).toHaveBeenCalled()
  })

  it('should call updatePreset and reload on editPreset', async () => {
    vi.mocked(presetsApi.listPresets).mockResolvedValue(mockPresets)
    const store = useSettingsStore()
    await store.editPreset('p1', { name: 'Renamed' })
    expect(presetsApi.updatePreset).toHaveBeenCalledWith('p1', { name: 'Renamed' })
    expect(presetsApi.listPresets).toHaveBeenCalled()
  })

  it('should call deletePreset and reload on removePreset', async () => {
    vi.mocked(presetsApi.listPresets).mockResolvedValue(mockPresets)
    const store = useSettingsStore()
    await store.removePreset('p2')
    expect(presetsApi.deletePreset).toHaveBeenCalledWith('p2')
    expect(presetsApi.listPresets).toHaveBeenCalled()
  })
})
