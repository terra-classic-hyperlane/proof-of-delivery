# Arquitetura — visão do todo

Diagramas do sistema completo: o processo Hyperlane de ponta a ponta e onde a
camada de remuneração (este repositório) se encaixa. Os diagramas renderizam no
GitHub (Mermaid). Detalhes normativos: `SPEC.html`.

---

## 1. Visão geral — as 4 redes e as duas camadas

O **core Hyperlane** (azul) não é modificado; a **camada de remuneração**
(verde) só muda configuração: o `beneficiary` de cada IGP vira o Vault e, em
Solana, o `owner` do IGP vira o governor.

```mermaid
flowchart TB
    subgraph TC["🌕 TERRA CLASSIC (colateral real · governança on-chain)"]
        direction TB
        TCmb["Mailbox<br/>(grava sender+block no DELIVERIES)"]
        TCigp["IGP<br/>claim só p/ beneficiary"]
        TCora["StorageGasOracle<br/>(contrato separado)"]
        TCwarp["Warp Route<br/>colateral LUNC"]
        TCvault["🟢 RelayerRewardVault<br/>prova por RAW QUERY"]
        TCgov["🟢 OracleGovernor<br/>quórum+mediana+faixa"]
        TCigp -- "beneficiary = vault" --> TCvault
        TCvault -- "raw query DELIVERIES" --> TCmb
        TCgov -- "owner do oracle" --> TCora
    end

    subgraph EVM["🟡 BSC · 🔵 ETHEREUM (sintético · multisig)"]
        direction TB
        Emb["Mailbox v3<br/>processor(id) público"]
        Eigp["IGP<br/>claim() permissionless"]
        Eora["StorageGasOracle"]
        Ewarp["Warp sintético"]
        Evault["🟢 RelayerRewardVault.sol<br/>prova por processor()"]
        Egov["🟢 GasOracleGovernor.sol"]
        Eigp -- "empurra saldo (receive)" --> Evault
        Evault -- "processor(id)" --> Emb
        Egov -- "owner do oracle" --> Eora
    end

    subgraph SOL["🟣 SOLANA (sintético · multisig · SEM registro de executor)"]
        direction TB
        Smb["Mailbox<br/>ProcessedMessage sem executor"]
        Sigp["IGP<br/>(oracle DENTRO do Igp)"]
        Swarp["Warp sintético"]
        Svault["🟢 rrv: PDA config = POOL<br/>quórum por ÉPOCA"]
        Sgov["🟢 IgpOracleGovernor<br/>duas portas · owner do IGP"]
        Sigp -- "beneficiary = PDA do vault" --> Svault
        Sgov -- "owner (CPI assinada pela PDA)" --> Sigp
    end

    TCwarp <-. "mensagens Hyperlane<br/>(validators + relayers)" .-> Ewarp
    TCwarp <-. " " .-> Swarp
```

---

## 2. O processo Hyperlane completo (com a remuneração acoplada)

Uma transferência TC → BSC, do dispatch ao saque do relayer — os passos 1–8 são
o **core inalterado**; 9–11 são a **camada nova**:

```mermaid
sequenceDiagram
    autonumber
    actor U as Usuário
    participant WO as Warp Route (TC)
    participant MO as Mailbox (TC)
    participant IO as IGP (TC)
    participant V as Validators (assinam checkpoints)
    actor R as Relayer (qualquer um!)
    participant MD as Mailbox (BSC)
    participant ISM as ISM (BSC)
    participant WD as Warp sintético (BSC)
    participant VD as Vault (BSC)

    U->>WO: transfer(1000 LUNC, dest=BSC)
    WO->>IO: quote + payForGas (LUNC do usuário)
    Note over IO: arrecadação fica no IGP<br/>até alguém puxar p/ o beneficiary
    WO->>MO: dispatch(mensagem)
    MO-->>V: novo checkpoint (merkle root)
    V-->>V: assinam o checkpoint (threshold ex.: 3-de-4)
    R->>MD: process(metadata, mensagem)
    MD->>ISM: verify(assinaturas)
    ISM-->>MD: ✓ válido
    MD->>WD: handle() → cunha sintético p/ o destinatário
    Note over MD: Mailbox grava Delivery{processor=R, block}
    R->>VD: claim([message_id])
    VD->>MD: processor(id) == R? processedAt+janela ok?
    VD-->>R: 💰 recompensa em BNB (do pool)
```

O caminho de volta (BSC → TC) é espelhado: o usuário paga BNB no IGP da BSC, o
relayer gasta LUNC no `process()` do TC e saca do **Vault do TC** — cada rede
se sustenta na própria moeda (spec §05).

---

## 3. Fluxo do dinheiro (autofinanciado por construção)

```mermaid
flowchart LR
    U1["Usuários que despacham<br/>DA rede X"] -- "payForGas<br/>(moeda de X)" --> IGP["IGP da rede X"]
    IGP -- "claim()/Sweep<br/>(saldo inteiro)" --> POOL["🟢 Vault da rede X<br/>(pool na moeda de X)"]
    POOL -- "reward × entregas<br/>comprovadas" --> REL["Relayers que ENTREGARAM<br/>NA rede X"]
    GOVx["governança/multisig"] -. "WithdrawSurplus /<br/>tarifa / janela" .-> POOL
    style POOL fill:#0a6b4e,color:#fff
```

Sem uso → sem arrecadação, mas também sem trabalho. Tarifa < arrecadação média
por entrega ⇒ o pool nunca fica insolvente (spec §01/§05).

**v2 (ClaimRemote):** o pool da rede de ORIGEM também paga a taxa da mensagem ao
operador que a entregou NUMA OUTRA rede — via atestação com quórum (diagrama 4b):

```mermaid
flowchart LR
    U2["Usuário despacha<br/>DE X para Y"] -- "taxa (moeda de X)" --> IGPX["IGP de X"] --> POOLX["Vault de X"]
    RELY["SEU relayer<br/>entrega em Y"] -- "verificada pelo<br/>claim-agent" --> ATT["AttestRemoteDelivery<br/>no Vault de X"]
    ATT -- "vínculo + quórum +<br/>1x por message_id" --> POOLX
    POOLX -- "recompensa remota<br/>(≈ taxa de origem)" --> OP["Operador<br/>(endereço em X)"]
    style POOLX fill:#0a6b4e,color:#fff
```

---

## 4. Prova de entrega — os três mecanismos

```mermaid
flowchart TB
    subgraph P1["TERRA CLASSIC · prova por raw query"]
        A1["Relayer chama Vault::Claim{ids}"] --> B1["raw query: chave<br/>[0x00,0x0A]+'deliveries'+id"]
        B1 --> C1{"valor parseia<br/>{sender, block_number}?"}
        C1 -- "não" --> D1["⛔ MailboxLayoutMismatch<br/>(migrate detectado — nunca paga errado)"]
        C1 -- "sim" --> E1{"sender == relayer?<br/>dentro da janela?<br/>não pago ainda?"}
        E1 -- "sim (TODOS os ids)" --> F1["💰 BankMsg (lote atômico)"]
        E1 -- "não" --> G1["⛔ reverte o LOTE inteiro"]
    end

    subgraph P2["BSC/ETHEREUM · prova direta"]
        A2["claim(ids)"] --> B2["mailbox.processor(id) == msg.sender?<br/>processedAt(id)+janela ≥ bloco?"]
        B2 -- sim --> C2["💰 transfer nativo (atômico, reentrancy-guard)"]
    end

    subgraph P4["QUALQUER ORIGEM · v2 ClaimRemote (atestação de entrega REMOTA)"]
        A4["claim-agent verifica a entrega<br/>na chain de DESTINO"] --> B4["AttestRemoteDelivery{domínio, ids}<br/>no vault da chain de ORIGEM"]
        B4 --> C4{"atestador registrado?<br/>vínculo (operador,domínio)?<br/>id nunca pago?"}
        C4 -- sim --> D4{"atestações CONCORDANTES<br/>≥ quórum?"}
        D4 -- sim --> E4["💰 recompensa fixa do domínio<br/>ao operador vinculado (1x por id)"]
        D4 -- "ainda não" --> F4["atestação registrada,<br/>aguarda quórum (auditável)"]
        C4 -- não --> G4["⛔ reverte"]
    end

    subgraph P3["SOLANA · quórum por época (a chain não registra o executor)"]
        A3["Época de 6h fecha<br/>(+ folga de finalidade)"] --> B3["cada operador submete o relatório:<br/>janela + créditos ORDENADOS por chave"]
        B3 --> C3{"hashes idênticos<br/>≥ quórum?"}
        C3 -- sim --> D3["créditos atribuídos por operador<br/>(PDA OperatorCredit)"]
        C3 -- "divergência" --> E3["🚨 época travada — alarme,<br/>auditoria pública dos relatórios"]
        D3 --> F3["Withdraw: débito de lamports<br/>do pool (respeita rent-exempt)"]
    end
```

---

## 5. Organograma de governança (spec §04)

Três esferas com escopos que não se sobrepõem — o operacional com quem opera, o
estrutural com quem tem mandato:

```mermaid
flowchart TB
    GOV["🏛️ GOVERNANÇA DO TERRA CLASSIC<br/>proposta on-chain<br/><i>escopo: TUDO dentro do TC —<br/>IGP · ISM · Vault · Oracle · tarifa · faixa</i>"]
    MS["🔐 MULTISIG (BSC · ETH · SOL)<br/>validadores Hyperlane + signatários EXTERNOS<br/><i>escopo: IGP · ISM · faixa do oracle ·<br/>upgrade authority (Solana) · emergências</i>"]
    OPS["⚙️ OPERADORES DE RELAYER<br/>quórum on-chain (sem multisig de carteira)<br/><i>escopo: preço DENTRO da faixa · relatórios<br/>de época (SOL) · parâmetros do vault remoto</i>"]
    ALL["🌐 QUALQUER UM (permissionless)<br/><i>entregar mensagens · sacar a PRÓPRIA<br/>recompensa · Sweep · auditar tudo</i>"]

    GOV -- "dá mandato por<br/>proposta aprovada" --> MS
    MS -- "define faixa e travas<br/>(nunca os operadores)" --> OPS
    OPS -.-> ALL
    style GOV fill:#B58CFF,color:#000
    style MS fill:#F0B90B,color:#000
    style OPS fill:#3FD0C9,color:#000
```

> ⚠️ O risco nº 1 do projeto mora aqui: quem controla o **ISM do Warp remoto**
> tem acesso indireto ao colateral do TC (cunhar sintético sem lastro → queimar
> → mensagem legítima libera colateral). Por isso o multisig **nunca** pode ser
> composto só pelos validadores que assinam checkpoints, e a troca de ISM pede
> timelock (spec §12).

---

### 5b. ClaimRemote — quem controla o quê (v2)

```mermaid
flowchart TB
    OWN["Owner do vault<br/>(hoje deployer · depois governança/multisig)"]
    OWN -->|SetRemoteOperators| ATT["Atestadores + quórum<br/>(hoje: 1 operador, quórum 1)"]
    OWN -->|SetRemoteBinding| BIND["Vínculos de identidade<br/>operador ↔ endereço em cada chain"]
    OWN -->|SetRemoteReward| RW["Recompensa fixa por domínio<br/>(≈ taxa média de origem)"]
    ATT -->|AttestRemoteDelivery| PAY["Pagamento 1x por message_id<br/>(effects-first, auditável)"]
    BIND --> PAY
    RW --> PAY
    NOTE["⚠️ com 1 operador o quórum 1 é auto-atestação<br/>(fase de teste) — subir p/ ≥2 com operadores independentes"]
    ATT -.-> NOTE
```

## 6. Oracle de preço — o mesmo padrão nas 4 redes

```mermaid
flowchart LR
    subgraph DEFINE["governança (TC) / multisig (remotas)"]
        FX["faixa [min,max] por domínio<br/>+ variação máx (bps)<br/>+ token_decimals (SOL)"]
    end
    subgraph OPS["operadores (cada um com seu oracle-agent)"]
        OA["agente A"] & OB["agente B"] & OC["agente C"]
    end
    G["🟢 GOVERNOR<br/>mediana (menor dos centrais)<br/>na faixa? Δ < limite?<br/>1 aplicação por época"]
    ORC["oracle da rede<br/>(StorageGasOracle / gas_oracles do Igp)"]
    QUOTE["quotes do IGP<br/>(o que o usuário paga)"]

    FX -- "trava on-chain" --> G
    OA & OB & OC -- "SubmitPrice<br/>(observação independente)" --> G
    G -- "CPI / setRemoteGasData" --> ORC --> QUOTE
    DEFINE -. "EMERGÊNCIA: escrita direta<br/>+ devolução da posse" .-> ORC
    style G fill:#0a6b4e,color:#fff
```

O conflito de interesse (operadores controlam o preço que financia a própria
remuneração) é neutralizado porque a **faixa é definida por quem não opera**.

---

## 7. Solana — as duas portas do IgpOracleGovernor

Em Solana o oracle vive DENTRO do `Igp` e só o owner escreve — o governor vira
o owner e reconstrói a separação de poderes:

```mermaid
flowchart TB
    subgraph GOVSOL["🟢 IgpOracleGovernor (owner do Igp via config PDA)"]
        P1["PORTA 1 · operadores<br/>SubmitPrice → quórum → mediana<br/>→ faixa/delta → CPI"]
        P2["PORTA 2 · multisig (1 assinatura)<br/>SetDomainConfig (faixa+decimals)<br/>SetIgpBeneficiary<br/>ForceSetGasData<br/><b>TransferIgpOwnership ← saída de emergência</b>"]
    end
    IGPS["Igp { owner, beneficiary, gas_oracles }"]
    P1 -- "SetGasOracleConfigs<br/>(assinada pela config PDA)" --> IGPS
    P2 -- "CPIs administrativas" --> IGPS
    MS2["multisig"] --> P2
    MS2 -- "upgrade authority<br/>do programa" --> GOVSOL
```

A separação passa a ser **lógica** (código do governor), não criptográfica —
por isso as três obrigações da spec §08: `TransferIgpOwnership` testada antes
do deploy (✅ testada em `svm/.../tests`), lamports mantidos na config PDA, e
upgrade authority no multisig.
