from app.services.docx_export import markdown_to_docx


def test_markdown_to_docx_produces_docx_bytes():
    buffer = markdown_to_docx("# 标题\n\n- 事项")
    assert buffer.getvalue().startswith(b"PK")
