# 首轮发布文案

状态：供维护者在 YouTube 等平台单独发布的文案。视频、字幕、音频与演示素材不进入 Git、不由 Web 或 GitHub Release 托管；发布前自行替换平台链接并检查成片。

## A：90 秒主片

**标题：我做了个开源工具，把 AI 聊天变成语言练习**

**封面：边聊，边懂，边记住。**

简介：

> 用 AI 练英语时，我经常遇到一个表达：大概看懂了，但轮到自己说，又想不起来。
>
> 所以做了 Parley。左边用目标语言聊天，右边随时解释、翻译或问“怎么说”。想留下的表达，连同原句一起收藏，再写下自己的理解，之后继续复习。
>
> 视频用一个做饭场景展示完整过程。画面是实际界面与本地示例响应，经过剪辑，不代表实时模型速度；口播为本地合成语音。
>
> 项目开源免费。Web 使用自己的 API；桌面还提供本机 CLI 和终端伴随入口，助手模型独立配置。
>
> Web：https://krahsu.github.io/parley/
>
> 源码：https://github.com/KraHsu/parley
>
> 试着留下你今天想学的一个表达。

置顶评论：

> 开始使用：打开 Web，在设置里添加 API 服务，为主聊和语言助手选好模型。浏览器直连需要服务允许 CORS，具体步骤见仓库 `docs/WEB.md`。没有 API 凭据也可以手动添加词句、打开复习。
>
> 如果卡住了，欢迎记录系统、使用入口、操作步骤和错误现象；不要附上 API Key。

## B：30 秒短片

**标题：这句英语，聊完以后怎么留下来？**

**封面：遇到一个表达，就把它留下来。**

简介：

> 选中一个不会的表达，在旁边问明白，再带着原句收藏。
>
> Parley 是我做的开源语言学习工作台：对话 → 解释 → 收藏 → 复习。
>
> Web 使用自己的 API。视频为实际界面与示例响应，经过剪辑，口播为合成语音。
>
> 体验：https://krahsu.github.io/parley/
>
> 源码：https://github.com/KraHsu/parley

## C：60 秒终端静态讲解

**标题：在 Codex 旁边，放一个语言助手**

封面标记：**终端伴随 · 静态讲解**

简介：

> 继续使用自己的原生 CLI，把语言方面的问题留给旁边的 GUI。
>
> Parley 的 `parley-cli` 提供 Codex / Claude Code 终端入口；已保存的消息可同步到 GUI，在 GUI 正文里选词、解释、翻译和收藏。登录、快捷键与原生历史仍由 CLI 管理，语言助手单独选择服务。
>
> 这支使用当前启动器帮助输出与已有 GUI 验证记录做静态讲解，不是本次现场生成。答疑材料含示例响应；口播为合成语音。不同 CLI 的同步范围详见终端指南。
>
> 终端指南：https://github.com/KraHsu/parley/blob/main/docs/TERMINAL_COMPANION.md
>
> 源码：https://github.com/KraHsu/parley

## D：四张图文卡片

**标题：聊过的英语，怎么变成自己的表达？**

> 用 AI 练英语，难的不只是开始聊天。有些表达看得懂，轮到自己说却想不起来。
>
> Parley 把这个过程分成几步：用目标语言分享自己的生活；选中不懂的词句，在旁边问词义和语法；把值得留下的表达连同原句收藏，再补上自己的释义和注释；之后打开复习，先回忆，再看答案。
>
> 也可以把母语里的想法交给助手，问“怎么说更自然”。主聊和答疑分开，原来的聊天记录还在。
>
> 软件开源免费。Web 使用自己的 API，学习资料保存在当前浏览器；桌面还有终端伴随入口。图片使用示例数据。
>
> 体验：https://krahsu.github.io/parley/
>
> 源码：https://github.com/KraHsu/parley

## 仓库 About

Description：

```text
Turn conversations into language practice. Open-source web and desktop workspace for chat, explanations, vocabulary and review, with API and native CLI backends.
```

Homepage：`https://krahsu.github.io/parley/`

Topics：`language-learning`、`vocabulary`、`spaced-repetition`、`tauri`、`vue`、`rust`、`llm`、`codex`、`claude-code`。
