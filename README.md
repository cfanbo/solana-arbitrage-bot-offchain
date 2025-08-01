一款基于 [jup.ag](https://jup.ag/) 实现的套利软件。

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

为了发现套利的机会，需要频繁的访问jup服务进行询价，因此会造成服务端返回 `429 Too Many Requests` 错误，因此引入了多IP支持功能，这些IP以轮训的方式工作，如果IP数量过多的话，可以通过减少 `frequency` 的值来实现更频繁的报价请求，增加发现套利的机会。

配置示例

```
frequency = 1000 # 每秒询价一次， 共发送两个HTTP请求
ips = "4.4.4.4,8.8.8.8"
```

表示每秒进行一次询价，每次询价需要发送两次HTTP请求，而每次请求使用其中一个IP地址。


> [!NOTE]
> 更多配置项介绍可参考文件里的注释，默认配置 `simulate_transaction = true` 表示模拟交易，如果在生产环境使用，请设置为 `false`

## 注意事项

由于token的价格在不停的实时变化，因此运行此软件时，需要确保与JUP API（配置项
`jup_v6_api_base_url`）的网速质量非常的高，才可以大大的提高套利成功的机率，否则可能导致套利失败，并造成一定程序的损失。
如果启用了`jito`提交交易的话，最好也保证与服务器之间的网速足够的快。
