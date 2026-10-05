// Disposable PostgreSQL schema and two real application processes behind one
// round-robin local origin. Forwarding never retries writes or uses sticky sessions.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),http=require('node:http'),crypto=require('node:crypto');
const {spawn,spawnSync}=require('node:child_process');
const assert=require('node:assert/strict');
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
module.exports=async function fixture(temporary){
 const root=process.env.TEST_DATABASE_URL;assert(root,'Local process browser journey requires TEST_DATABASE_URL');
 const psql=process.env.WPALT_TEST_PSQL||'psql',schema='wpalt_browser_'+crypto.randomBytes(12).toString('hex');
 function sql(text){const r=spawnSync(psql,[root,'-X','-qAt','-v','ON_ERROR_STOP=1','-c',text],{encoding:'utf8'});assert.equal(r.status,0,'Disposable PostgreSQL browser fixture failed');return r.stdout;}
 sql('CREATE SCHEMA '+schema);
 const database=new URL(root);database.searchParams.set('options','-c search_path='+schema);
 database.search=database.search.replace(/\+/g,'%20');
 const databaseUrl=database.toString();
 const children=[],fds=[],nodes=[],counts=[0,0],staticFailures=[];let proxy;
 async function stop(child){if(child.exitCode!==null||child.signalCode!==null)return;child.kill('SIGTERM');await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{child.kill('SIGKILL');reject(Error('Local process did not drain'));},65000);child.once('exit',(code)=>{clearTimeout(timer);code===0?resolve():reject(Error('Local process exited unsuccessfully'));});});}
 return {databaseUrl,async start(binary,config,publicPort){
  const source='local_processes=true\n'+fs.readFileSync(config,'utf8');
  for(let i=0;i<2;i++){
   const nodePort=await port(),cfg=path.join(temporary,`node-${i}.toml`);fs.writeFileSync(cfg,source.replace(/^listen\s*=.*$/m,`listen="127.0.0.1:${nodePort}"`),{mode:0o600});
   const fd=fs.openSync(path.join(temporary,`node-${i}.log`),'w',0o600);fds.push(fd);
   const child=spawn(binary,['--config',cfg,'serve','--external-worker'],{stdio:['ignore',fd,fd]});children.push(child);nodes.push(nodePort);
   let ready=false;for(let n=0;n<150;n++){assert.equal(child.exitCode,null,'Local process exited before readiness');try{if((await fetch(`http://127.0.0.1:${nodePort}/health`)).ok){ready=true;break;}}catch{}await new Promise(r=>setTimeout(r,100));}assert(ready,'Local process readiness timed out');
  }
  const workerFd=fs.openSync(path.join(temporary,'worker.log'),'w',0o600);fds.push(workerFd);
  children.push(spawn(binary,['--config',path.join(temporary,'node-0.toml'),'worker'],{stdio:['ignore',workerFd,workerFd]}));
  let cursor=0;
  proxy=http.createServer((request,response)=>{
   const index=cursor++%nodes.length;counts[index]++;
   const upstream=http.request({hostname:'127.0.0.1',port:nodes[index],method:request.method,path:request.url,headers:request.headers},incoming=>{if(incoming.statusCode>=400 && request.url.startsWith("/assets/"))staticFailures.push({path:request.url.split("?")[0],status:incoming.statusCode,node:index});response.writeHead(incoming.statusCode,incoming.headers);incoming.pipe(response);});
   upstream.on('error',(error)=>{if(request.url.startsWith('/assets/'))staticFailures.push({path:request.url.split('?')[0],error:error.code||'transport',node:index});if(!response.headersSent){response.writeHead(502,{'Content-Type':'text/plain','Cache-Control':'no-store'});response.end('Local node unavailable; operation is not retried.');}else response.destroy();});
   request.pipe(upstream);
  });await new Promise(r=>proxy.listen(publicPort,'127.0.0.1',r));
 },async close(){
  if(proxy){await new Promise(r=>{proxy.close(r);proxy.closeAllConnections();});}
  let failure;for(const child of children){try{await stop(child);}catch(e){failure=e;}}
  for(const fd of fds)fs.closeSync(fd);
  fs.writeFileSync(path.join(temporary,"../m9-browser-transport.json"),JSON.stringify({counts,staticFailures},null,2));
  sql('DROP SCHEMA '+schema+' CASCADE');
  if(failure)throw failure;
 },report(){assert(counts.every(n=>n>100),'Cumulative browser requests must exercise both nodes');return {processes:2,workers:1,routing:'round-robin-no-retry-no-stickiness',requests_by_node:counts};}};
};
