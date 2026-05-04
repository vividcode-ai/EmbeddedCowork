# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-blue)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-blue)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-blue)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-blue)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-blue)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-blue)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-red)](README.he.md)

## תא הטייס של AI ל-OpenCode

EmbeddedCowork הופכת את OpenCode מכלי טרמינל ל**סביבת עבודה שולחנית פרימיום** — שנבנתה עבור מפתחים שחיים בתוך סשנים של תכנות עם AI במשך שעות וזקוקים לשליטה, מהירות ובהירות.

> OpenCode נותן לך את המנוע. EmbeddedCowork נותן לך את תא הטייס.

![סביבת עבודה מרובת מופעים](docs/screenshots/newSession.png)

---

## תכונות

- **🚀 סביבת עבודה מרובת מופעים**
- **🌐 גישה מרחוק**
- **🧠 ניהול סשנים**
- **🎙️ קלט קולי ודיבור**
- **🌳 Git Worktrees**
- **💬 חוויית הודעות עשירה**
- **🧩 SideCars**
- **⌨️ לוח פקודות**
- **📁 סייר קבצים**
- **🔐 אימות ואבטחה**
- **🔔 התראות**
- **🎨 עיצובים**
- **🌍 בינלאומיות**

---

## תחילת העבודה

### 🖥️ אפליקציה שולחנית

זמינה בגרסאות Electron ו-Tauri — בחר לפי העדפתך.

הורד את המתקין העדכני ביותר לפלטפורמה שלך מ-[Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases).

| פלטפורמה | פורמטים |
|-----------|---------|
| macOS | DMG, ZIP (אוניברסלי: Intel + Apple Silicon) |
| Windows | מתקין NSIS, ZIP (x64, ARM64) |
| Linux | AppImage, deb, tar.gz (x64, ARM64) |

### 💻 שרת EmbeddedCowork

הרץ כשרת מקומי וגש דרך הדפדפן. מושלם לפיתוח מרחוק.

```bash
npx @vividcodeai/embeddedcowork --launch
```

ראה [תיעוד השרת](packages/server/README.md) עבור דגלים, TLS, אימות וגישה מרחוק.

### 🧪 גרסאות פיתוח

גרסאות מתקדמות מענף `dev`:

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## SideCars

SideCars מאפשרים לך לפתוח כלי אינטרנט מקומיים בתוך EmbeddedCowork כטאבים.

<details>
<summary><strong>תצורה</strong></summary>

- **שם**: שם התצוגה ב-EmbeddedCowork
- **פורט**: שירות HTTP או HTTPS מקומי הפועל על `127.0.0.1:<port>`
- **נתיב בסיס**: מותקן תחת `/sidecars/:id`
- **מצב קידומת**:
  - **שמור קידומת** מעביר את הנתיב המלא `/sidecars/:id/...` במעלה הזרם
  - **הסר קידומת** מסיר את `/sidecars/:id` לפני העברת הבקשה במעלה הזרם

</details>

<details>
<summary><strong>VSCode (שרת OpenVSCode)</strong></summary>

הרץ עם Docker:

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

הוסף SideCar בתור:

- **שם**: `VSCode`
- **פורט**: `http://127.0.0.1:8000`
- **נתיב בסיס**: `/sidecars/vscode`
- **מצב קידומת**: `שמור קידומת`

</details>

<details>
<summary><strong>טרמינל (ttyd)</strong></summary>

הרץ עם:

```bash
ttyd --writable zsh
```

הוסף SideCar בתור:

- **שם**: `Terminal`
- **פורט**: `http://127.0.0.1:7681`
- **נתיב בסיס**: `/sidecars/terminal`
- **מצב קידומת**: `הסר קידומת`

</details>

---

## דרישות מערכת

- **[OpenCode CLI](https://opencode.ai)** — חייב להיות מותקן וב-`PATH` שלך
- **Node.js 18+** — עבור מצב שרת או בנייה מקוד מקור

---

## פיתוח

EmbeddedCowork הוא מונוריפו הבנוי עם:

| חבילה | תיאור |
|-------|--------|
| **[packages/server](packages/server/README.md)** | לוגיקה ראשית ו-CLI — סביבות עבודה, פרוקסי OpenCode, API, אימות, דיבור |
| **[packages/ui](packages/ui/README.md)** | צד קדמי ב-SolidJS — ראקטיבי, מהיר, יפהפה |
| **[packages/electron-app](packages/electron-app/README.md)** | מעטפת שולחנית — ניהול תהליכים, IPC, דיאלוגים מקוריים |
| **[packages/tauri-app](packages/tauri-app)** | מעטפת שולחנית Tauri (ניסיוני) |

### התחלה מהירה

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## פתרון בעיות

<details>
<summary><strong>macOS: "EmbeddedCowork.app פגום ולא ניתן לפתוח אותו"</strong></summary>

דגל Gatekeeper עקב חוסר באישור notarization. נקה את תכונת ההסגר:

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

ב-Intel Mac, בדוק גם **הגדרות מערכת → פרטיות ואבטחה** בהפעלה הראשונה.
</details>

<details>
<summary><strong>Linux (Wayland + NVIDIA): אפליקציית Tauri נסגרת מיד</strong></summary>

בעיית WebKitGTK DMA-BUF/GBM. הרץ עם:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

ראה את הפתרון המלא ב-README המקורי.
</details>

---

## קהילה

[![היסטוריית כוכבים](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

## תודות

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad)
- [opencode](https://github.com/anomalyco/opencode)

**נבנה עם ♥ על ידי [VividCodeAI](https://github.com/vividcode-ai)** · [רישיון MIT](LICENSE)
