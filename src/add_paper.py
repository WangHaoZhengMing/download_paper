import requests
from operations.download_page import download_page, question_page
from operations.model import question_page
import os
import uuid
import json
import asyncio
from playwright.async_api import Browser, Page
from operations.ask_llm_menshen import ask_llm_for_playload
from operations.connect_browser import connect_to_browser_and_page
import tomli_w
from qcloud_cos import CosConfig, CosS3Client
# --- START: 配置区 - 请根据需要修改 ---
API_BASE_URL = "https://tps-tiku-api.staff.xdf.cn"
AUTH_HEADERS = {
    "accept": "application/json, text/plain, */*",
    "content-type": "application/json",
    "cookie": "XDFUUID=26142d7c-eecc-a69d-8e72-9c1f4b2c0217; e2e=55B2D1619F0C8CF273169F8F1CA49A93; e2mf=51f0b63db37747ab82e172b74256783a; token=51f0b63db37747ab82e172b74256783a",
    "origin": "https://tk-lpzx.xdf.cn",
    "referer": "https://tk-lpzx.xdf.cn/",
    "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36 Edg/143.0.0.0"
}
NOTIFY_API_PATH = "/attachment/batch/upload/files" 

# --- END: 配置区 ---


async def get_upload_credentials(page, filename):
    """阶段1: 从你的服务器获取腾讯云COS的临时上传凭证。"""
    print("--- 阶段1: 正在请求上传凭证 (Via Page Fetch)... ---")
    
    js_code = f"""
    async (filename) => {{
        const url = "{API_BASE_URL}/attachment/get/credential";
        const payload = {{
            fileName: filename,
            contentType: "application/pdf",
            storageType: "cos",
            securityLevel: 1
        }};
        
        try {{
            const response = await fetch(url, {{
                method: "POST",
                headers: {{
                    "Content-Type": "application/json",
                    "Accept": "application/json, text/plain, */*",
                    "tikutoken": "732FD8402F95087CD934374135C46EE5"
                }},
                credentials: "include",
                body: JSON.stringify(payload)
            }});
            
            const data = await response.json();
            return data;
        }} catch (e) {{
            console.error("Fetch error:", e);
            return {{ success: false, message: e.toString() }};
        }}
    }}
    """
    
    try:
        response_data = await page.evaluate(js_code, filename)
        
        if response_data and response_data.get('success'):
            print("✅ 凭证获取成功。")
            return response_data.get('data')
        else:
            print(f"❌ 错误: API响应格式不正确或未成功。")
            print("   服务器响应:", response_data)
            return None
            
    except Exception as e:
        print(f"❌ 错误: 获取凭证失败: {e}")
        return None

def upload_to_cos(credentials_data, file_path):
    """阶段2: 使用临时凭证将文件直接上传到腾讯云COS。"""
    print("\n--- 阶段2: 正在上传文件到腾讯云COS... ---")
    creds = credentials_data['credentials']
    config = CosConfig(
        Region=credentials_data['region'], SecretId=creds['tmpSecretId'],
        SecretKey=creds['tmpSecretKey'], Token=creds['sessionToken'], Scheme='https'
    )
    client = CosS3Client(config)
    
    bucket = credentials_data['bucket']
    key_prefix = credentials_data['keyPrefix']
    filename = os.path.basename(file_path)
    object_key = f"{key_prefix}/{uuid.uuid4()}/{filename}"
    
    print(f"   云端路径 (Key): {object_key}")
    try:
        client.upload_file(Bucket=bucket, LocalFilePath=file_path, Key=object_key)
        final_url = f"https://{credentials_data['cdnDomain']}/{object_key}"
        print("✅ 文件上传成功。")
        print(f"   最终文件URL: {final_url}")
        return {"url": final_url, "key": object_key}
    except Exception as e:
        print(f"❌ 错误: 上传到COS失败: {e}")
        return None

async def notify_application_server(page, filename, file_info):
    """阶段3: 通知你的服务器上传已完成，并获取处理结果。"""
    print("\n--- 阶段3: 正在通知应用服务器 (Via Page Fetch)... ---")
    
    js_code = f"""
    async (data) => {{
        const url = "{API_BASE_URL}{NOTIFY_API_PATH}";
        const payload = {{
            "uploadAttachments": [
                {{
                    "fileName": data.filename,
                    "fileType": "pdf",
                    "fileUrl": data.fileUrl,
                    "resourceType": "zbtiku_pc"
                }}
            ],
            "fileUploadType": 5,
            "fileContentType": 1,
            "paperId": ""
        }};
        
        try {{
            const response = await fetch(url, {{
                method: "POST",
                headers: {{
                    "Content-Type": "application/json",
                    "Accept": "application/json, text/plain, */*",
                    "tikutoken": "732FD8402F95087CD934374135C46EE5"
                }},
                credentials: "include",
                body: JSON.stringify(payload)
            }});
            
            const resData = await response.json();
            return resData;
        }} catch (e) {{
            console.error("Fetch error:", e);
            return {{ success: false, message: e.toString() }};
        }}
    }}
    """
    
    try:
        data = {"filename": filename, "fileUrl": file_info['url']}
        response_data = await page.evaluate(js_code, data)
        
        print("✅ 服务器通知成功，已收到返回数据。")
        return response_data
    except Exception as e:
        print(f"❌ 错误: 通知服务器失败: {e}")
        return None


async def upload_pdf(page, file_path)->str:
    if not os.path.exists(file_path):
        print(f"❌ 错误: 文件 '{file_path}' 不存在，请先创建。")
        return

    filename = os.path.basename(file_path)
    
    credentials = await get_upload_credentials(page, filename)
    if not credentials:
        return

    file_info = upload_to_cos(credentials, file_path)
    if not file_info:
        return
        
    final_result = await notify_application_server(page, filename, file_info)
    if not final_result:
        return
        
    if final_result.get("success") and "data" in final_result:
        data_array = final_result["data"]
        print("\n" + "="*50)
        print("🎉 成功获取到目标 `data` 数组! 🎉")
        return '"attachments": ' + str(json.dumps(data_array, indent=2, ensure_ascii=False))
    else:
        print("\n❌ 未能从最终响应中找到 'data' 数组。服务器返回内容如下:")
        print(json.dumps(final_result, indent=2, ensure_ascii=False))

async def save_new_paper(question_page, tiku_page: Page)->str:
    
    payload = await ask_llm_for_playload(f"$Question_name: {question_page.name} + Subject: {question_page.subject} + Province: {question_page.province}")
    parcial_payload = await upload_pdf(tiku_page, f"PDF/{question_page.name}.pdf")

    # Properly construct the JSON payload by parsing and merging
    # Remove trailing comma if present to avoid JSON parsing errors
    payload = payload.rstrip().rstrip(',')
    
    try:
        payload_dict = json.loads('{' + payload + '}')
    except json.JSONDecodeError as e:
        print(f"JSON parsing error: {e}")
        print(f"Payload content: {payload}")
        raise

    # Parse parcial_payload which is in format '"attachments": [...]'
    if parcial_payload:
        # Extract key and value from the string
        key_value_parts = parcial_payload.split(':', 1)
        if len(key_value_parts) == 2:
            key = key_value_parts[0].strip().strip('"')
            value = json.loads(key_value_parts[1])
            payload_dict[key] = value

    payload_json = json.dumps(payload_dict, ensure_ascii=False)

    print(f"\n发送的payload: {payload_json}") 
    
    result = await tiku_page.evaluate(f"""
        fetch("https://tps-tiku-api.staff.xdf.cn/paper/new/save", {{
        method: "POST",
        headers: {{
            "Content-Type": "application/json",
            "Accept": "application/json, text/plain, */*"
        }},
        credentials: "include",
        body: {json.dumps(payload_json)}
        }})
        .then(res => res.json())
        .then(data => {{
            console.log("服务器返回：", data);
            return data;
        }})
        .catch(err => {{
            console.error(err);
            return {{ error: err.toString() }};
        }});
         """)
    
    print(f"API响应: {json.dumps(result, indent=2, ensure_ascii=False)}")
    
    if result and result.get("success"):
        paper_id = result.get("data")
        print(f"✅ 成功! 获取到的paper_id: {paper_id}")
        question_page.page_id = paper_id
        
        from pathlib import Path
        output_dir = Path("./output_toml")
        output_dir.mkdir(parents=True, exist_ok=True)
        toml_path = output_dir / f"{question_page.name}.toml"
        page_data_dict = {
            'name': question_page.name,
            'province': question_page.province,
            'grade': question_page.grade,
            'year': question_page.year,
            'subject': question_page.subject,
            'page_id': question_page.page_id if question_page.page_id else None,
            'stemlist': [{'origin': q.origin, 'stem': q.stem} for q in question_page.stemlist]
        }
        with open(toml_path, 'wb') as f:
            tomli_w.dump(page_data_dict, f)
        print(f"Saved TOML: {toml_path}")

        return paper_id
    else:
        print(f"❌ 请求失败或未返回成功状态")
        if result:
            print(f"   错误详情: {result}")
        return None


if __name__ == "__main__":
    async def main():
        browser: Browser
        page: Page
        browser, page = await connect_to_browser_and_page(target_url="https://zujuan.xkw.com/26p2916512.html",port=2001,target_title="")
        page_data = await download_page(page)

        # 注意：这里直接使用 page 作为 tiku_page 可能会因为跨域问题失败，
        # 仅作为测试代码修复参数缺失问题。实际运行时请确保 page 在正确的域。
        paper_id = await save_new_paper(page_data, page)
        
        # Clean up browser connection to avoid resource warnings
        await browser.close()
        return paper_id
    
    result = asyncio.run(main())
    print(f"\n最终结果: {result}")