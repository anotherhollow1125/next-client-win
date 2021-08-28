## Download



## Nextcloud Client for Windows

## 🚨 CAUTION 🚨

This app uses Basic Authentication to access your Nextcloud server. It means that if your server doesn't use SSL encryption, usernames and passwords will leak to attackers.

THEREFORE, **IF YOUR NEXTCLOUD SERVER's URL START "http://", YOU MUST NOT USE THIS APP!!!**

## How to use



ref: [Add an app to run automatically at startup in Windows 10](https://support.microsoft.com/en-us/windows/add-an-app-to-run-automatically-at-startup-in-windows-10-150da165-dcd9-7230-517b-cf3c295d89dd)

## Q&A



I'm Japanese, so I'll write a Japanese document below.

日本人なので以下に日本語のドキュメントも置いておきます。

## Nextcloud Client for Windows

[Nextcloud](https://nextcloud.com/)のWindows用クライアントアプリケーションです。バックグラウンドで動作し、設定したフォルダ上のファイルの追加、変更、削除等を検出してNextcloudのフォルダと双方向に同期します。

もちろん公式にも[クライアントアプリ](https://nextcloud.com/install/)は存在します。しかしこのアプリケーションでは除外ファイルリスト機能がうまく機能していないようで、除外ファイルの設定を行っても勝手に同期されてしまいます。

公式アプリに(すでに[issueはある](https://github.com/nextcloud/desktop/issues/2728)ようなので)プルリクエストでも送れば良かったのかもしれませんが、私にはC++を書く力はなかったため、代わりにRustで実装を行いました。そもそも、このアプリを作成した動機も「Rustプロジェクト内のtargetフォルダを同期させたくなかったから」です。

Nextcloud公式にこのアプリケーションの存在が知られたら嬉しいですね。

## 🚨 注意 🚨

このアプリはBasic認証を使用してNextcloudサーバーにアクセスします。つまり、SSL通信を用いない場合、ユーザー名とパスワードが攻撃者に漏洩します。

したがって、 **NextcloudサーバーのURLが"http://"で始まっているならばこのアプリケーションを絶対に使用しないでください。**

## 使い方

### 1. インストール

ダウンロードしたzipファイルを解凍し、「next_client_win.exe」を適当な場所に置いて使用してください。ダブルクリックで起動します。

Windowsのスタートアップ機能を使用するとWindows起動時に本アプリも立ち上げてもらえます。必ず設定してください。

参考: [Windows 10 の起動時に自動的に実行するアプリを追加する](https://support.microsoft.com/ja-jp/windows/windows-10-%E3%81%AE%E8%B5%B7%E5%8B%95%E6%99%82%E3%81%AB%E8%87%AA%E5%8B%95%E7%9A%84%E3%81%AB%E5%AE%9F%E8%A1%8C%E3%81%99%E3%82%8B%E3%82%A2%E3%83%97%E3%83%AA%E3%82%92%E8%BF%BD%E5%8A%A0%E3%81%99%E3%82%8B-150da165-dcd9-7230-517b-cf3c295d89dd)

1. 本アプリのショートカットを作成する
2. `Winキー + R` で「ファイル名を指定して実行」を起動し、 `shell:startup` と入力する
3. フォルダが開くので1で作成した本アプリのショートカットをそのフォルダに入れる

### 2. 初回起動時

初回起動前にマシンがインターネットに確実に接続されていることを確認してください。

初回起動時には、必要な情報の入力を求められます。入力した情報は本アプリが存在するフォルダの `conf.ini` に保存されます。

|項目|内容|
|:-:|:---|
|NC_HOST| あなたのNextcloudサーバーのURLを入力してください。 例: https://cloud.example.com/ |
|NC_USERNAME| ユーザー名を入力してください。 例: user |
|NC_PASSWORD| パスワードを入力してください。 例: password |
|LOCAL_ROOT| 同期させるフォルダのパスを入力してください。 例: c:/Users/user/Desktop/nextcloud |
|RUST_LOG| 出力されるログのレベルを設定できます。省略した場合はINFOです。 OFF, DEBUG, INFO, WARN, ERROR から選べます。 |

### 3. 通知領域アイコン

#### 3.1. アイコンの種類

本アプリを起動すると、通知領域(あるいはタスクトレイ等と呼ばれます。[参考](https://atmarkit.itmedia.co.jp/ait/articles/1604/19/news009.html))に本アプリの状態を示すアイコンが設置されます。

状態は4種類存在します。

|アイコン|説明|
|:-----:|:---|
| ![nc_normal](nc_normal.ico) | 正常時のアイコンです。 |
| ![nc_load](nc_load.ico) | ロード中のアイコンです。サーバーと通信中であったり、フォルダを操作している時にこのアイコンになります。ロード中に激しくファイル操作を行った場合、フォルダが破損する可能性があります。 |
| ![nc_offline](nc_offline.ico) | オフラインのアイコンです。マシンがネットワークに接続されていなかったり、サーバーにアクセスできない時にこのアイコンになります。ネットワークの接続を確認してください。また、初回起動時は設定のために確実にインターネットに接続されている必要があります。 |
| ![nc_error](nc_error.ico) | エラー時のアイコンです。 `show log` でどのようなエラーが発生しているかを確認し、直してください。 |

#### 3.1. アイコンによる操作 (コマンド)

アイコンをクリックするとコンテキストメニューが表示されます。コンテキストメニューにあるコマンドからアプリの操作が行えます。

|コマンド|説明|
|:-----:|:--|
|show log| ログファイルをnotepadで起動します。ログは `.ncs/log` に存在するファイルからも確認できます。 |
|edit conf.ini| 本アプリの設定ファイルをnotepadで起動します。 |
|edit excludes| `.ncs/excludes.json` ファイルをnotepadで起動します。 **正規表現で** 除外するファイル、除外しないファイルを設定できます。詳しくは「5. 除外設定」を確認してください。 |
|repair|フォルダの内容がサーバー上のものと一致するように修正を行います。ローカル上にのみ存在するファイルは、 `.ncs/stash` フォルダにバックアップを取った上で消去されます。|
|restart|本アプリを再起動します。|
|exit|本アプリを終了します。|

### 4. 同期用メタデータ

同期対象として設定されたフォルダには `.ncs` という隠しフォルダが生成されます。 `.ncs` フォルダは以下のような構成になっています。( `stash` は初めてファイルの退避が行われる時に生成されます。)

```
.ncs/
  ├── log/
  ├── stash/
  ├── cache.json
  └── excludes.json
```

|フォルダ/ファイル|説明|
|:--------------:|:--|
|log| ログファイルが格納されています。ログファイルの名前はアプリ起動時の日付となっています。 |
|stash| `repair` コマンド等で削除されたフォルダやファイルが時刻をファイル名の後ろにつけた形で退避されています。 |
|cache.json| 同期用のキャッシュデータが格納されています。編集しないでください。 |
|excludes.json| 同期から除外したいフォルダ/ファイルを |

### 5. 除外設定

`.ncs/excludes.json` は `blacks` フィールドと `wihtes` フィールドからなるJSONファイルです。 `edit excludes` コマンドからファイルを開くことができます。

`blacks` には同期したくないフォルダ/ファイル名を、 `whites` には `blacks` に引っかかるものの同期を行いたいフォルダ/ファイル名を、それぞれ **正規表現で** 設定します。同期しないと判断されたフォルダ/ファイルは、ローカルで作成されても無視され、別な方法でサーバー上に保存されてもローカルに保存されません。ブラックリストよりホワイトリストが優先され、例えば `\d+\.txt` を `blacks` に含めていても、 `whites` に `10.txt` が含まれていれば `10.txt` は同期されます。

あくまでも各フォルダ/ファイル名に対してのみチェックを行うので、「 `hoge/target` は同期したくないけど `fuga/target` は同期したい」というような設定は不可能です。ご了承ください。

`.` と `~` で始まるフォルダ/ファイル名は、 `blacks` 、 `whites` には明記されていませんがデフォルトで同期されません。

`whites` に `\.ncs` を含めることだけは絶対にやめてください。ログファイルが更新され続けるため無限ループとなりサーバーに多大な負荷がかかります。(どうしても同期したければ `RUST_LOG` を `OFF` に設定してください。その代償としてログファイルは完全に機能しません。)

## Q&A

### Q1. オフライン時もローカルでのファイル操作は記録されていますか？

A1. 変更があったファイルの記録は行っており、通信回復時に同期されますが、フォルダやファイルの移動などは記録していないため控えたほうが良いです。あくまでもオンライン時に使用するようにしてください。

### Q2. フォルダが壊れました

A2. 上述の通り `repair` を試してください。それでも不具合がある場合は、ディレクトリの中身を `.ncs` フォルダを含めすべて消去してください。(その場合、 `.ncs/excludes.json` 、すなわち除外設定も削除されることに気をつけてください。)

誤った操作でファイル等が消えた場合、サーバーの方のゴミ箱に残っている可能性があるので望みを捨てないでください。また、 `repair` コマンドを使用した場合などで消去されたローカルのファイルは `.ncs/stash` フォルダに保存されている可能性があるので、そちらも合わせて確認してください。

### Q3. ファイルが同期されない！/そもそもアプリが働いていない？

一度シャットダウンしませんでしたか？「初回起動時にスタートアップに本アプリを自動的に追加する」機能は実装されていません。スタートアップに登録しない場合毎回手動で起動する必要があります。「使い方」の1. インストールを参考に本アプリをスタートアップに登録し、Windows起動時に本アプリが起動するようにしてください。

### Q4. 一部ファイルが同期されない！/除外設定したファイルが同期される！

次の点を確認してください。

- `.` 、 `~` で始まるファイルは隠しファイルとみなしデフォルトで同期されません。特に `.gitignore` ファイルなどは注意が必要となります。
- 設定はすべて正規表現です。例えば `.gitignore` ファイルをホワイトリストに加えたい場合、`\.gitignore` と書かなければ `agitignore` 等をブラックリストに設定していても同期されてしまいます。
- ブラックリストよりホワイトリストが優先されます。
- パス全体で判断する機能はなく、単純にブラックリスト/ホワイトリストに追加された正規表現にマッチするファイル/フォルダは排除/同期されます。親フォルダがブラックリストに引っかかった場合、その子ファイルは同期されません。ご注意ください。
- リセット等を目的として `.ncs` フォルダを消去してしまった場合、 `excludes.json` も削除されるため、改めて設定する必要があります。

### Q5. 一部のコマンドが機能しない

`edit excludes` 等一部コマンドはエラー時にはクリックしても何も起きません。一度アプリを閉じ、再起動してください。

### Q6. 意味不明なエラーが発生した！

解決困難であればissueを立ててください。できる限り対応します。