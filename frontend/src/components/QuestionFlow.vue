<template>
  <div class="question-flow">
    <div class="question-flow-inner">
      <div class="question-card">
        <div class="question-meta">
          <div class="question-title">
            {{ questionnaire.title || t('questionnaire.title') }}
          </div>
          <div class="question-progress">
            {{ t('questionnaire.progress', { current: currentIndex + 1, total: totalQuestions }) }}
          </div>
        </div>

        <div class="question-header" v-if="currentQuestion?.header">
          {{ currentQuestion.header }}
        </div>
        <div class="question-text">
          {{ currentQuestion?.question || '' }}
          <span v-if="currentQuestion?.required" class="required-marker">*</span>
        </div>

        <div class="question-body">
          <template v-if="hasOptions">
            <el-checkbox-group
              v-if="Boolean(currentQuestion?.multiple)"
              v-model="currentResponse.selected_options"
              class="option-group"
            >
              <el-checkbox
                v-for="opt in currentQuestion?.options || []"
                :key="opt"
                :label="opt"
              >
                {{ opt }}
              </el-checkbox>
            </el-checkbox-group>

            <el-radio-group
              v-else
              v-model="singleChoice"
              class="option-group"
            >
              <el-radio
                v-for="opt in currentQuestion?.options || []"
                :key="opt"
                :value="opt"
              >
                {{ opt }}
              </el-radio>
            </el-radio-group>
          </template>

          <el-input
            v-model="currentResponse.free_text"
            type="textarea"
            :rows="3"
            :placeholder="currentQuestion?.placeholder || t('questionnaire.freeTextPlaceholder')"
          />

          <el-input
            v-model="currentResponse.notes"
            type="textarea"
            :rows="2"
            :placeholder="t('questionnaire.notesPlaceholder')"
          />
        </div>

        <div class="question-actions">
          <el-button
            data-testid="question-prev"
            :disabled="currentIndex === 0 || disabled"
            @click="goPrevious"
          >
            {{ t('questionnaire.previous') }}
          </el-button>
          <el-button
            v-if="!isLastQuestion"
            data-testid="question-next"
            type="primary"
            :disabled="disabled || !isCurrentValid"
            @click="goNext"
          >
            {{ t('questionnaire.next') }}
          </el-button>
          <el-button
            v-else
            data-testid="question-submit"
            type="primary"
            :disabled="disabled || !isCurrentValid"
            @click="submit"
          >
            {{ t('questionnaire.submit') }}
          </el-button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { ActiveQuestionnaire, QuestionAnswer } from '../types'
import { t } from '../i18n'

const props = defineProps<{
  questionnaire: ActiveQuestionnaire
  disabled?: boolean
}>()

const emit = defineEmits<{
  submit: [answers: QuestionAnswer[]]
}>()

type QuestionResponse = {
  selected_options: string[]
  free_text: string
  notes: string
}

const currentIndex = ref(0)
const responses = ref<Record<string, QuestionResponse>>({})

function ensureResponse(questionId: string): QuestionResponse {
  if (!responses.value[questionId]) {
    responses.value[questionId] = {
      selected_options: [],
      free_text: '',
      notes: '',
    }
  }
  return responses.value[questionId]
}

watch(
  () => props.questionnaire,
  (value) => {
    currentIndex.value = 0
    const next: Record<string, QuestionResponse> = {}
    for (const q of value.questions) {
      next[q.id] = {
        selected_options: [],
        free_text: '',
        notes: '',
      }
    }
    responses.value = next
  },
  { immediate: true, deep: true },
)

const totalQuestions = computed(() => props.questionnaire.questions.length)
const currentQuestion = computed(() => props.questionnaire.questions[currentIndex.value])
const currentResponse = computed(() => {
  const q = currentQuestion.value
  if (!q) return { selected_options: [], free_text: '', notes: '' }
  return ensureResponse(q.id)
})

const hasOptions = computed(() => (currentQuestion.value?.options?.length || 0) > 0)
const isLastQuestion = computed(() => currentIndex.value >= totalQuestions.value - 1)

const singleChoice = computed<string>({
  get() {
    return currentResponse.value.selected_options[0] || ''
  },
  set(value: string) {
    currentResponse.value.selected_options = value ? [value] : []
  },
})

function isQuestionValid(index: number): boolean {
  const q = props.questionnaire.questions[index]
  if (!q) return false
  if (!q.required) return true
  const response = ensureResponse(q.id)
  const hasSelection = response.selected_options.length > 0
  const hasText = response.free_text.trim().length > 0
  return hasSelection || hasText
}

const isCurrentValid = computed(() => isQuestionValid(currentIndex.value))

function goPrevious() {
  if (currentIndex.value > 0) {
    currentIndex.value -= 1
  }
}

function goNext() {
  if (!isCurrentValid.value) return
  if (currentIndex.value < totalQuestions.value - 1) {
    currentIndex.value += 1
  }
}

function submit() {
  if (!isCurrentValid.value) return
  const answers: QuestionAnswer[] = props.questionnaire.questions.map((q) => {
    const r = ensureResponse(q.id)
    return {
      id: q.id,
      question: q.question,
      selected_options: [...r.selected_options],
      free_text: r.free_text,
      notes: r.notes,
    }
  })
  emit('submit', answers)
}
</script>

<style scoped>
.question-flow {
  padding: 12px 16px 20px;
  flex-shrink: 0;
}

.question-flow-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
}

.question-card {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-lg);
  background: var(--bg-input);
  box-shadow: var(--shadow-sm);
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.question-meta {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
}

.question-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.question-progress {
  font-size: 12px;
  color: var(--text-secondary);
}

.question-header {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.question-text {
  font-size: 15px;
  color: var(--text-primary);
}

.required-marker {
  color: #dc2626;
  margin-left: 4px;
}

.question-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.option-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.question-actions {
  display: flex;
  justify-content: space-between;
}
</style>
