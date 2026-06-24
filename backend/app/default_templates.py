from app.constants import ReportType


DEFAULT_TEMPLATES = {
    ReportType.WEEKLY: {
        "name": "默认周报模板",
        "content": """# {{ title }}

周期：{{ period_start }} - {{ period_end }}

{{ ai_content }}

## 原始工作记录

{{ work_items }}
""",
    },
    ReportType.MONTHLY: {
        "name": "默认月报模板",
        "content": """# {{ title }}

周期：{{ period_start }} - {{ period_end }}

{{ ai_content }}

## 关键成果

{{ highlights }}

## 原始工作记录

{{ work_items }}
""",
    },
    ReportType.PERFORMANCE: {
        "name": "默认绩效考核模板",
        "content": """# {{ title }}

考核周期：{{ period_start }} - {{ period_end }}

{{ ai_content }}

## 贡献证据

{{ highlights }}

## 可改进事项

{{ blockers }}
""",
    },
}
