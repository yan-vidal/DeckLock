# Roadmap / Próximos passos

Directions, not delivery promises. / Direções, sem promessa de datas.

## 0.2.0 — released 2026-09-09 / publicada em 2026-09-09

- Procedural media, live editors and preview diagnostics / mídias procedurais, editores ao vivo e métricas.
- Consistent ordinary window controls and bilingual offline help / controles consistentes e ajuda offline bilíngue.
- Reviewed changelog, full version names and documented release checks / changelog revisado, versões completas e processo de release documentado.
- PAM notice reliability: bounded helper output and elapsed-time countdowns / confiabilidade dos avisos PAM: saída limitada do auxiliar e contagem por tempo decorrido.

For 0.2.0, keep the existing system PAM integration (`pam_service = "login"` by default). Do not install a dedicated `/etc/pam.d/decklock` policy or edit system authentication rules. A dedicated service may be reconsidered later if a concrete packaging or policy need arises; it is not required for the current helper architecture.
Na 0.2.0, manter a integração atual com o PAM do sistema (`pam_service = "login"` por padrão). Não instalar política própria em `/etc/pam.d/decklock` nem editar regras de autenticação do sistema. Um serviço específico pode ser reconsiderado se surgir uma necessidade concreta de empacotamento ou política; a arquitetura atual do auxiliar não depende dele.

## Next direction — platform reach before new features / Próxima direção — alcance de plataforma antes de novas funcionalidades

Broaden the systems DeckLock runs on before adding features. The project is installable in practice only on Arch today, so a new feature is built for an audience that cannot install it, and each feature multiplies the per-platform validation cost of everything that follows. The disposable QEMU gate with real Sway and real PAM, built for 0.2.0, is the reusable asset that makes this cheap. Discussed in issues #7-#19; no version or date is assigned.
Ampliar os sistemas onde o DeckLock roda antes de acrescentar funcionalidades. Hoje o projeto só é instalável na prática no Arch, então uma funcionalidade nova é construída para um público que não consegue instalá-la, e cada funcionalidade multiplica o custo de validação por plataforma de tudo que vem depois. O gate descartável em QEMU com Sway e PAM reais, feito para a 0.2.0, é o ativo reaproveitável que torna isso barato. Discutido nas issues #7-#19; sem versão ou data atribuída.

- Phase A — more Wayland compositors (#7, #8, #9): verification only, same binary and same package / mais compositores Wayland: só verificação, mesmo binário e mesmo pacote.
- Phase B — Fedora and Debian/Ubuntu packages with their own recorded dependency baselines, per-distribution `pam_service` and lockout messages, and the disposable VM gate per distribution (#10-#14) / pacotes Fedora e Debian/Ubuntu com baseline de dependências próprio, `pam_service` e mensagens de bloqueio por distribuição, e o gate de VM por distribuição.
- Phase C — X11 lock backend behind a compatibility layer (#15-#18) / backend de bloqueio X11 atrás de uma camada de compatibilidade.
- Phase D — other Unix systems; the portability boundary is authentication and power, not graphics (#19) / outros sistemas Unix; a fronteira de portabilidade é autenticação e energia, não gráficos.

Widgets/plugins, remote media and the greeter stay behind this reach work.
Widgets/plugins, mídias remotas e greeter ficam atrás desse trabalho de alcance.

## Candidates for later / Candidatos para depois

- Real-device performance and battery measurements / medições de desempenho e bateria no dispositivo.
- More compositor/controller recovery and accessibility coverage / ampliar validação de recuperação e acessibilidade.
- More original media and theme customization / novas mídias autorais e personalização de temas.
- A documentation website built from docs/guide / site de documentação a partir de docs/guide.
- Start with built-in Rust widgets (for example clock and battery), then allow plugins to add new widget types. Lua is the proposed extension language, subject to isolation design and validation; built-in widgets need not depend on Lua. / Começar com widgets nativos em Rust (por exemplo relógio e bateria), depois permitir novos tipos por plugins. Lua é a linguagem proposta para extensões, sujeita ao projeto e à validação do isolamento; widgets nativos não precisam depender de Lua.
- Plugin design must restrict capabilities, CPU and memory, isolate execution, and protect authentication from overlays and input capture. Plugins must never receive credentials or authorize unlock. / O projeto de plugins deve restringir permissões, CPU e memória, isolar a execução e proteger a autenticação de sobreposições e captura de entrada. Plugins nunca recebem credenciais nem autorizam desbloqueio.
- Optional remote media provider with local cache and offline fallback / provedor opcional de mídias remotas com cache local e alternativa offline.
- Investigate a greetd greeter sharing themes and media while keeping login/session management separate / investigar greeter para greetd compartilhando temas e mídias, com gerenciamento de login e sessão separado.

Widgets/plugins, remote media and the greeter are outside 0.2.0; no later version or delivery date is assigned.
Widgets/plugins, mídias remotas e greeter ficam fora da 0.2.0; sem versão posterior ou data de entrega definida.

Use issues to discuss concrete work before assigning a version. X11 locking is accepted for a later version behind an explicit compatibility layer, with the reduced guarantee shown in the UI at lock time and not only in the documentation: an X11 session cannot isolate keystrokes, so another application in the session can read what is typed. Wayland stays the recommended path and the only one with the full guarantee.
Use issues para discutir tarefas concretas antes de atribuir uma versão. O bloqueio X11 está aceito para uma versão futura atrás de uma camada de compatibilidade explícita, com a garantia reduzida exibida na interface no momento do bloqueio e não apenas na documentação: uma sessão X11 não isola as teclas digitadas, então outro aplicativo da sessão pode ler o que é digitado. O Wayland segue sendo o caminho recomendado e o único com a garantia completa.
