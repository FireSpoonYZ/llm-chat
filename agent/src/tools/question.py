from __future__ import annotations

import asyncio
import json
import uuid
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import AliasChoices, BaseModel, Field, PrivateAttr, model_validator

from .result_schema import make_tool_error, make_tool_success


class QuestionItem(BaseModel):
    id: str | None = Field(default=None, description="Stable question identifier.")
    header: str | None = Field(default=None, description="Short header shown above the question.")
    question: str = Field(description="Question text shown to the user.")
    options: list[str | dict[str, Any]] = Field(
        default_factory=list,
        description="Optional list of choices for this question.",
    )
    placeholder: str | None = Field(
        default=None,
        description="Optional input placeholder for free-text responses.",
    )
    multiple: bool = Field(
        default=False,
        description="Whether multiple options can be selected.",
        validation_alias=AliasChoices("multiple", "multiSelect"),
    )
    required: bool = Field(
        default=True,
        description="Whether the question requires an answer before submit.",
    )


class QuestionInput(BaseModel):
    question: str | None = Field(
        default=None,
        description="Single-question shortcut. Use `questions` for multi-question flows.",
    )
    options: list[str | dict[str, Any]] = Field(
        default_factory=list,
        description="Single-question options when using the shortcut form.",
    )
    placeholder: str | None = Field(
        default=None,
        description="Single-question placeholder when using the shortcut form.",
    )
    multiple: bool = Field(
        default=False,
        description="Single-question multi-select flag when using the shortcut form.",
        validation_alias=AliasChoices("multiple", "multiSelect"),
    )
    required: bool = Field(
        default=True,
        description="Single-question required flag when using the shortcut form.",
    )
    title: str | None = Field(
        default=None,
        description="Optional title for a multi-question flow.",
        validation_alias=AliasChoices("title", "body"),
    )
    questions: list[QuestionItem] = Field(
        default_factory=list,
        description="Question list for multi-question flows.",
    )

    @model_validator(mode="after")
    def validate_input(self) -> "QuestionInput":
        has_single = bool((self.question or "").strip())
        has_multiple = len(self.questions) > 0
        if not has_single and not has_multiple:
            raise ValueError("Provide either `question` or `questions`.")
        if has_single and has_multiple:
            raise ValueError("Provide only one of `question` or `questions`, not both.")
        return self


class QuestionTool(BaseTool):
    """Ask structured questions to the user and wait for answers."""

    name: str = "question"
    supports_runtime_events: bool = True
    description: str = (
        "Ask one or more structured questions during execution to clarify "
        "requirements, preferences, and implementation choices. Collects user "
        "answers and then continues execution automatically."
    )
    args_schema: Type[BaseModel] = QuestionInput
    _event_sink: Any = PrivateAttr(default=None)
    _pending_answers: dict[str, asyncio.Future[list[dict[str, Any]]]] = PrivateAttr(default_factory=dict)
    _early_answers: dict[str, list[dict[str, Any]]] = PrivateAttr(default_factory=dict)
    _known_questionnaires: set[str] = PrivateAttr(default_factory=set)

    def set_event_sink(self, sink: Any) -> None:
        self._event_sink = sink

    def submit_answer(self, questionnaire_id: str, answers: list[dict[str, Any]]) -> bool:
        """Submit user answers to a pending questionnaire."""
        if questionnaire_id not in self._known_questionnaires:
            return False
        normalized_answers = self._normalize_answers(answers)
        fut = self._pending_answers.get(questionnaire_id)
        if fut is not None and not fut.done():
            fut.set_result(normalized_answers)
            return True

        # Handle race: answer arrives before _arun starts awaiting.
        self._early_answers[questionnaire_id] = normalized_answers
        return True

    def _normalize_questions(
        self,
        *,
        question: str | None = None,
        options: list[str | dict[str, Any]] | None = None,
        placeholder: str | None = None,
        multiple: bool = False,
        required: bool = True,
        questions: list[QuestionItem] | None = None,
    ) -> list[dict[str, Any]]:
        def _option_label(option: str | dict[str, Any]) -> str:
            if isinstance(option, str):
                return option
            if isinstance(option, dict):
                for key in ("label", "value", "title"):
                    value = option.get(key)
                    if isinstance(value, str) and value.strip():
                        return value
            return str(option)

        normalized: list[dict[str, Any]] = []
        source = list(questions or [])
        if not source and question:
            source = [
                QuestionItem(
                    question=question,
                    options=list(options or []),
                    placeholder=placeholder,
                    multiple=multiple,
                    required=required,
                )
            ]

        for idx, item in enumerate(source, start=1):
            q_id = (item.id or "").strip() or f"q{idx}"
            normalized.append({
                "id": q_id,
                "header": item.header,
                "question": item.question,
                "options": [_option_label(opt) for opt in item.options],
                "placeholder": item.placeholder,
                "multiple": bool(item.multiple),
                "required": bool(item.required),
            })
        return normalized

    def _normalize_answers(self, answers: list[dict[str, Any]]) -> list[dict[str, Any]]:
        def _string_or_empty(value: Any) -> str:
            if value is None:
                return ""
            if isinstance(value, str):
                return value
            return str(value)

        normalized: list[dict[str, Any]] = []
        for item in answers:
            if not isinstance(item, dict):
                continue
            selected_options = item.get("selected_options")
            if not isinstance(selected_options, list):
                selected_options = []
            normalized.append({
                "id": _string_or_empty(item.get("id", "")),
                "question": _string_or_empty(item.get("question", "")),
                "selected_options": [_string_or_empty(opt) for opt in selected_options if opt is not None],
                "free_text": _string_or_empty(item.get("free_text", "")),
                "notes": _string_or_empty(item.get("notes", "")),
            })
        return normalized

    def _run(self, **_: Any) -> dict[str, Any]:
        return make_tool_error(
            kind=self.name,
            error="question tool is async-only",
            text="Error: question tool requires async execution.",
        )

    async def _arun(
        self,
        question: str | None = None,
        options: list[str | dict[str, Any]] | None = None,
        placeholder: str | None = None,
        multiple: bool = False,
        required: bool = True,
        title: str | None = None,
        questions: list[QuestionItem] | None = None,
    ) -> dict[str, Any]:
        normalized_questions = self._normalize_questions(
            question=question,
            options=options,
            placeholder=placeholder,
            multiple=multiple,
            required=required,
            questions=questions,
        )
        questionnaire_id = f"qq-{uuid.uuid4().hex}"
        self._known_questionnaires.add(questionnaire_id)

        try:
            if self._event_sink is not None:
                await self._event_sink({
                    "type": "question",
                    "data": {
                        "questionnaire_id": questionnaire_id,
                        "title": title,
                        "questions": normalized_questions,
                    },
                })

            answers = self._early_answers.pop(questionnaire_id, None)
            if answers is None:
                loop = asyncio.get_running_loop()
                fut: asyncio.Future[list[dict[str, Any]]] = loop.create_future()
                self._pending_answers[questionnaire_id] = fut
                try:
                    answers = await fut
                except asyncio.CancelledError:
                    return make_tool_error(
                        kind=self.name,
                        error="question flow cancelled",
                        text="Error: question flow was cancelled before receiving answers.",
                    )
                finally:
                    self._pending_answers.pop(questionnaire_id, None)
        except Exception as exc:
            return make_tool_error(
                kind=self.name,
                error=f"question flow failed: {exc}",
                text="Error: question flow failed before receiving answers.",
            )
        finally:
            self._known_questionnaires.discard(questionnaire_id)
            self._early_answers.pop(questionnaire_id, None)

        payload = {
            "questionnaire_id": questionnaire_id,
            "answers": answers,
        }
        text = json.dumps(payload, ensure_ascii=False)
        return make_tool_success(
            kind=self.name,
            text=text,
            data={
                "questionnaire_id": questionnaire_id,
                "title": title,
                "questions": normalized_questions,
                "answers": answers,
            },
            meta={
                "question_count": len(normalized_questions),
                "answer_count": len(answers),
            },
        )
