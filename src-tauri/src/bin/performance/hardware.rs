use super::{Collected, Updates};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{mpsc, watch},
};

async fn stop_child(child: &mut Child, input: &mut ChildStdin) -> anyhow::Result<()> {
    let _ = input.write_all(b"stop\n").await;
    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(result) => {
            let status = result?;
            if !status.success() {
                anyhow::bail!("硬件采集退出异常：{status}");
            }
            Ok(())
        }
        Err(_) => {
            let _ = child.kill().await;
            anyhow::bail!("硬件采集停止超时，请检查采集进程");
        }
    }
}
fn status(state: &str, message: &str) -> Collected {
    Collected {
        hardware_status: Some(state.into()),
        message: Some(message.into()),
        ..Default::default()
    }
}
pub async fn run(
    tx: Updates,
    mut authorize: mpsc::Receiver<()>,
    mut stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let directory = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("插件目录无效"))?
        .join("sensors/publish");
    let executable = directory.join("m18-sensors.exe");
    let mut elevated = false;
    loop {
        if *stop.borrow() {
            return Ok(());
        }
        let mut command = Command::new(&executable);
        command
            .current_dir(&directory)
            .arg("--parent")
            .arg(std::process::id().to_string());
        if elevated {
            command.arg("--authorize");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let spawned = command.spawn();
        if let Ok(mut child) = spawned {
            let mut input = child.stdin.take().unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap());
            let restart = loop {
                let mut line = Vec::new();
                tokio::select! {
                    _=stop.changed()=> {stop_child(&mut child,&mut input).await?;return Ok(());},
                    Some(())=authorize.recv()=> {stop_child(&mut child,&mut input).await?;break true;},
                    result=async {
                        // Bound allocation before parsing a helper line, as on the plugin transport.
                        loop {
                            let bytes=output.fill_buf().await?;
                            if bytes.is_empty() {return Ok::<bool,std::io::Error>(false);}
                            let count=bytes.iter().position(|b| *b==b'\n').map(|n| n+1).unwrap_or(bytes.len());
                            if line.len()+count>1_048_576 {return Err(std::io::Error::other("硬件采集消息过大"));}
                            line.extend_from_slice(&bytes[..count]); output.consume(count);
                            if line.last()==Some(&b'\n') {return Ok(true);}
                        }
                    } => match result {
                        Ok(true)=>match serde_json::from_slice::<Collected>(&line) {
                            Ok(data)=> {if tx.send(("hardware",data)).await.is_err() {stop_child(&mut child,&mut input).await?;return Ok(());}},
                            Err(_)=> {let _=stop_child(&mut child,&mut input).await; let _=tx.send(("hardware",status("error","硬件采集返回无效数据"))).await;break false;}
                        },
                        _=> {let _=stop_child(&mut child,&mut input).await; let _=tx.send(("hardware",status("error","硬件采集进程已退出，可点击重试"))).await;break false;}
                    }
                }
            };
            if restart {
                elevated = true;
                continue;
            }
        } else {
            let _ = tx
                .send((
                    "hardware",
                    status("unavailable", "硬件采集组件缺失或无法启动，请重新安装"),
                ))
                .await;
        }
        // A failure requires an explicit retry, never a process restart loop.
        tokio::select! {_=stop.changed()=>return Ok(()),Some(())=authorize.recv()=>elevated=true}
    }
}
