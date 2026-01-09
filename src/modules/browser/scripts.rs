pub const ELEMENTS_DATA_JS: &str = r#"
        () => {
            const styles = Array.from(document.styleSheets)
                .map(sheet => {
                    try {
                        return Array.from(sheet.cssRules)
                            .map(rule => rule.cssText)
                            .join('\n');
                    } catch (e) {
                        return '';
                    }
                })
                .join('\n');
            const container = document.querySelector('.sec-item') ||
                            document.querySelector('.paper-content') ||
                            document.querySelector('body');
            if (!container) {
                return { styles: styles, elements: [] };
            }
            const allElements = Array.from(container.querySelectorAll('.sec-title, .sec-list'));
            const elements = [];
            allElements.forEach(el => {
                if (el.classList.contains('sec-title')) {
                    const span = el.querySelector('span');
                    const titleText = span ? span.innerText.trim() : '';
                    if (titleText) {
                        elements.push({
                            type: 'title',
                            title: titleText,
                            content: ''
                        });
                    }
                } else if (el.classList.contains('sec-list')) {
                    elements.push({
                        type: 'content',
                        title: '',
                        content: el.outerHTML
                    });
                }
            });
            return { styles: styles, elements: elements };
        }
    "#;

pub const TITLE_JS: &str = r#"
        () => {
            const titleElement = document.querySelector('.title-txt .txt');
            return titleElement ? titleElement.innerText : '未找到标题';
        }
    "#;

pub const INFO_JS: &str = r#"
        () => {
            const items = document.querySelectorAll('.info-list .item');
            if (items.length >= 2) {
                return {
                    shengfen: items[0].innerText.trim(),
                    nianji: items[1].innerText.trim()
                };
            }
            return { shengfen: '未找到', nianji: '未找到' };
        }
    "#;

pub const SUBJECT_JS: &str = r#"
        () => {
            const subjectElement = document.querySelector('.subject-menu__title .title-txt');
            return subjectElement ? subjectElement.innerText.trim() : '未找到科目';
        }
    "#;
