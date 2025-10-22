一款基于 [jup.ag](https://jup.ag/) 实现的套利软件，支持闪电贷。

> 本人无法保证使用此软件一定可以套利成功，因此若您使用了此软件，则视为愿意自行承担任何风险，谢谢！

## 支持命令

```shell
$ arbitrage-bot -h
一款基于 Jupiter Aggregator 实现的套利工具

Usage: arbitrage-bot [COMMAND]

Commands:
  version  打印版本信息
  update   检查并更新到最新版本
  run      运行套利主程序
  init     初始化配置文件
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## 使用教程

生成配置文件，两种方法

```shell
$ arbitrage-bot init
或
$ cp config.example.toml config.toml
```

并将配置文件 `config.toml` 与 `arbitrage-bot` 可执行文件放在同一目录。

再次编辑配置文件，填写 `private_key` 、`rpc_endpoint`
、`input_mint`、`output_mint` `input_amount` 和 `slippage_bps`等信息.

最后执行以下命令启动服务

```shell
$ arbitrage-bot
```

或

```shell
$ arbitrage-bot run
```

## 高级配置

### 多IP支持

为了发现套利的机会，需要频繁的访问jup服务进行询价，因此会造成服务端返回 `429 Too Many Requests` 错误，因此引入了多IP支持功能，这些IP以轮训的方式工作，如果IP数量过多的话，可以通过减少 `frequency` 的值来实现更频繁的报价请求，增加发现套利的机会。

配置示例

```
frequency = 1000 # 每秒询价一次， 共发送两个HTTP请求
ips = "4.4.4.4,8.8.8.8"
```

表示每秒进行一次询价，每次询价需要发送两次HTTP请求，而每次请求使用其中一个IP地址。

### 闪电贷

目前闪电贷平台仅支持 `kamino`。

在进行利润判断时，将闪电贷利息(手续费)计算在内。假如不考虑贷款利息的话，是存在利润的，但减去利息后则将亏损，此时则视为无套利空间，此时`利润保护合约`将此笔套利交易进行rollback

> [!NOTE]
>
> 启用闪电贷后，系统将根据用户账户余额决定是否进行贷款，如果账户余额大于 `input.amount`，则优先使用账户余额，否则将贷款。


> [!NOTE]
> 更多配置项介绍可参考文件里的注释，默认配置 `simulate_transaction = true` 表示模拟交易，如果在生产环境使用，请设置为 `false`

## 注意事项

由于token的价格在不停的实时变化，因此运行此软件时，需要确保与JUP API（配置项
`jup_v6_api_base_url`）的网速质量非常的高，才可以大大的提高套利成功的机率，否则可能导致套利失败，并造成一定程序的损失。
如果启用了`jito`提交交易的话，最好也保证与服务器之间的网速足够的快。

## 常见问题

1. 本软件是否收费

不收取任何费用，代码完全开源

2. 软件使用的合约是否开源

为了防止套利失败造成损失，当前使用了利润保护合约，它同样是开源的 https://github.com/cfanbo/https://github.com/cfanbo/profit_protect_program ，用户在使用本软件前，请先进行部署此合约，并更新 `profit_protect_program_id` 配置



## 联系作者

- GitHub: [@cfanbo](https://github.com/cfanbo)
- Telegram: @mark_guest
