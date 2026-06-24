from enum import Enum


class ReportType(str, Enum):
    WEEKLY = "weekly_report"
    MONTHLY = "monthly_report"
    PERFORMANCE = "performance_review"


class TaskStatus(str, Enum):
    PENDING = "pending"
    SUCCESS = "success"
    FAILED = "failed"
    SKIPPED = "skipped"


class ReportStatus(str, Enum):
    DRAFT = "draft"


class LLMProvider(str, Enum):
    OPENAI = "openai"
    NVIDIA = "nvidia"
    OPENROUTER = "openrouter"


PROVIDER_DEFAULT_BASE_URLS = {
    LLMProvider.OPENAI: "https://api.openai.com/v1",
    LLMProvider.NVIDIA: "https://integrate.api.nvidia.com/v1",
    LLMProvider.OPENROUTER: "https://openrouter.ai/api/v1",
}
