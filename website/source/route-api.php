<?php
/* VeloPopuli route planner, MIT. Fixed upstreams, cached responses and request limits. */
declare(strict_types=1);
ini_set('display_errors', '0');
header('Content-Type: application/json; charset=utf-8');
header('X-Content-Type-Options: nosniff');
header('Cache-Control: no-store');
function reply_error(string $message, int $status=400): void {
    http_response_code($status); echo json_encode(['error'=>$message]); exit;
}
if ($_SERVER['REQUEST_METHOD'] !== 'POST') reply_error('Use the route planner to make a request.',405);
if ((int)($_SERVER['CONTENT_LENGTH']??0)>12000) reply_error('Request too large.',413);
$input=json_decode(file_get_contents('php://input'),true);
if (!is_array($input)) reply_error('Invalid request.');
$mode=$input['mode']??'';
// Change these fixed URLs here if moving to self-hosted services.
$services=['search'=>'https://photon.komoot.io/api/', 'route'=>'https://brouter.de/brouter'];
if (!isset($services[$mode])) reply_error('Unknown planner operation.');
$params=[];
if ($mode==='search') {
    $q=trim((string)($input['q']??''));
    if (strlen($q)<3 || strlen($q)>180) reply_error('Search for a town or place using 3–180 characters.');
    $params=['q'=>$q,'limit'=>5,'lang'=>'en'];
} else {
    $points=$input['points']??[];
    if (!is_array($points)||count($points)<2||count($points)>9) reply_error('Choose between 2 and 9 route points.');
    $pairs=[];$prev=null;
    foreach($points as $p) {
        if (!is_array($p)||count($p)!==2||!is_numeric($p[0])||!is_numeric($p[1])) reply_error('Invalid route coordinate.');
        $lon=(float)$p[0];$lat=(float)$p[1];
        if (!is_finite($lon)||!is_finite($lat)||abs($lon)>180||abs($lat)>85) reply_error('Route coordinate out of range.');
        if ($prev!==null && (abs($lat-$prev[1])>3 || abs($lon-$prev[0])*cos(deg2rad($lat))>4)) reply_error('Keep route points within a regional ride.');
        $prev=[$lon,$lat];$pairs[]=sprintf('%.6F,%.6F',$lon,$lat);
    }
    $profile=($input['surface']??'road')==='mixed'?'trekking':'fastbike';
    $params=['lonlats'=>implode('|',$pairs),'profile'=>$profile,'alternativeidx'=>0,'format'=>'geojson','timode'=>0,
      'profile:allow_steps'=>'0','profile:allow_ferries'=>'0','profile:allow_motorways'=>'0',
      'profile:correctMisplacedViaPoints'=>'0', 'profile:consider_traffic'=>empty($input['quiet'])?'0.1':'1'];
    if (($input['terrain']??'')==='flat') { $params['profile:consider_elevation']='1';$params['profile:uphillcost']='100';$params['profile:downhillcost']='100'; }
    elseif (($input['terrain']??'')==='climb') $params['profile:consider_elevation']='0';
}
$cacheDir=sys_get_temp_dir().'/velopopuli-planner-v1';
if (!is_dir($cacheDir) && !@mkdir($cacheDir,0700,true)) reply_error('Planner cache is unavailable. Please try again later.',503);
$key=hash('sha256',$mode.json_encode($params));$path=$cacheDir.'/'.$key.'.json';
$ttl=$mode==='search'?604800:86400;
if (is_file($path) && time()-filemtime($path)<$ttl) {readfile($path);exit;}
// One upstream request at a time per service; a short bounded wait, never a request fan-out.
$lock=fopen($cacheDir.'/'.$mode.'.lock','c+');
if (!$lock || !flock($lock,LOCK_EX|LOCK_NB)) reply_error('The community routing service is busy. Please try again in a moment.',429);
try {
    // Protect the public proxy from a single client consuming the shared service.
    $client=hash('sha256',($_SERVER['REMOTE_ADDR']??'unknown').date('Y-m-d-H').(string)floor((int)date('i')/10));
    $rateFile=$cacheDir.'/rate-'.$client;
    $count=is_file($rateFile)?(int)file_get_contents($rateFile):0;
    if ($count>=35) reply_error('You have made several requests. Please wait a few minutes before generating more.',429);
    file_put_contents($rateFile,(string)($count+1),LOCK_EX);
    rewind($lock);$last=(float)stream_get_contents($lock);$delay=1.1-(microtime(true)-$last);
    if ($delay>0) usleep((int)($delay*1000000));
    $url=$services[$mode].'?'.http_build_query($params,'','&',PHP_QUERY_RFC3986);
    $ua='VeloPopuliRoutePlanner/1.0 (+https://velopopuli.com/route-planner.html)';
    if (function_exists('curl_init')) {
        $ch=curl_init($url);
        curl_setopt_array($ch,[CURLOPT_RETURNTRANSFER=>true,CURLOPT_CONNECTTIMEOUT=>10,CURLOPT_TIMEOUT=>45,CURLOPT_USERAGENT=>$ua,CURLOPT_HTTPHEADER=>['Accept: application/json'],CURLOPT_FOLLOWLOCATION=>false]);
        $body=curl_exec($ch);$status=(int)curl_getinfo($ch,CURLINFO_RESPONSE_CODE);curl_close($ch);
    } else {
        $ctx=stream_context_create(['http'=>['timeout'=>45,'header'=>"User-Agent: $ua\r\nAccept: application/json\r\n",'ignore_errors'=>true,'follow_location'=>0]]);
        $body=@file_get_contents($url,false,$ctx);$status=isset($http_response_header[0])?(int)substr($http_response_header[0],9,3):0;
    }
    ftruncate($lock,0);rewind($lock);fwrite($lock,(string)microtime(true));
    if ($body===false || $status!==200 || strlen($body)>12000000) {
        $detail=$mode==='route'&&is_string($body)&&strlen($body)<600?trim(strip_tags($body)):'';
        reply_error($mode==='route'?'The road service could not route these points (HTTP '.$status.'). '.($detail!==''?substr($detail,0,220):'Try moving points closer to roads or choosing another direction.'):'Place search is unavailable. You can still select points on the map.',502);
    }
    $data=json_decode($body,true);
    if (!is_array($data)||!isset($data['features'])||!is_array($data['features'])) reply_error('The mapping service returned an unexpected result. Please try again.',502);
    $out=json_encode($data,JSON_UNESCAPED_UNICODE|JSON_UNESCAPED_SLASHES);
    $temp=tempnam($cacheDir,'new-');if ($temp!==false) {file_put_contents($temp,$out);rename($temp,$path);}
    // Bound disk use; delete only this planner's old cache/rate files.
    if (random_int(1,30)===1) foreach(glob($cacheDir.'/*')?:[] as $f) if(is_file($f)&&substr($f,-5)!=='.lock'&&time()-filemtime($f)>604800) @unlink($f);
    echo $out;
} catch (Throwable $e) {reply_error('The routing service could not complete this request. Please try again.',503);}
finally {if(is_resource($lock)){flock($lock,LOCK_UN);fclose($lock);}}
