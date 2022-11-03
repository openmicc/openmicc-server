[
  {
    name = "openssl";
    replacements = [
      {
        url = "https://www.openssl.org/source/openssl-3.0.2.tar.gz";
        sha256 = "sha256-mOkczq1NR1auPJzeXgkZGo5YbZ9NUIOOfsCdZBHf22M=";
      }
      {
        url = "https://wrapdb.mesonbuild.com/v2/openssl_3.0.2-1/get_patch";
        sha256 = "sha256-diq06pTQIXjWodPrY0CcLE1hMV01g5HNrGLfFSERdNQ=";
      }
    ];
  }
  {
    name = "abseil-cpp";
    replacements = [
      {
        url = "https://github.com/abseil/abseil-cpp/archive/20211102.0.tar.gz";
        sha256 = "sha256-3PcbnLqNwMqZQMSzFqDHlr6Pq0KwcLtrfKtitI8OZsQ=";
      }
      {
        url =
          "https://wrapdb.mesonbuild.com/v2/abseil-cpp_20211102.0-2/get_patch";
        sha256 = "sha256-lGOTA2ew25hENTUMfXYU5AD6qIEafpot71pj/zn9syU=";
      }
    ];
  }
  {
    name = "catch2";
    replacements = [
      {
        url = "https://github.com/catchorg/Catch2/archive/v2.13.7.zip";
        sha256 = "sha256-PzzNkK06j7sb7rFebbRAzNy+vjeN/RJdB6H5pYepJ+k=";
      }
      {
        url = "https://wrapdb.mesonbuild.com/v2/catch2_2.13.7-1/get_patch";
        sha256 = "sha256-L3NpZF10flvYZjF6wd1MPQTcl9Oq1PxrhkvfddO1cVg=";
      }
    ];
  }
  {
    name = "libsrtp2";
    replacements = [{
      url = "https://github.com/cisco/libsrtp/archive/refs/tags/v2.4.2.zip";
      sha256 = "sha256-NbGuemJWIk/rBY8f60IXBTekSJY0D4Dne0nMWa9oaoI=";
    }];
  }
  {
    name = "libuv";
    replacements = [
      {
        url = "https://dist.libuv.org/dist/v1.44.1/libuv-v1.44.1.tar.gz";
        sha256 = "sha256-nTe2NDD+O5KpOGuUm+vY8LR4SjmhaWTILJVmJHp29ko=";
      }
      {
        url = "https://wrapdb.mesonbuild.com/v2/libuv_1.44.1-1/get_patch";
        sha256 = "sha256-ihBRWM2ryipU8cfMTC+BTBWSceENxeN+0aCPE8/Wf/c=";
      }
    ];
  }
  {
    name = "nlohmann_json";
    replacements = [{
      url =
        "https://github.com/nlohmann/json/releases/download/v3.10.5/include.zip";
      sha256 = "sha256-uUmX32iFZ1O3Lw16NwO31ITUdFxWfzWE75fJbCWleY4=";
    }];
  }
  {
    name = "usrsctp";
    replacements = [{
      url =
        "https://github.com/sctplab/usrsctp/archive/4e06feb01cadcd127d119486b98a4bd3d64aa1e7.zip";
      sha256 = "sha256-FfeETExMqTIorg/oRBgscu3R2Am0YcuXsbtoeoBN1Pw=";
    }];
  }
  {
    name = "wingetopt";
    replacements = [{
      url = "https://github.com/alex85k/wingetopt/archive/v1.00.zip";
      sha256 = "sha256-RFTKA6WXAqTKTRSIyo+mFosMjXfcc5pv4oJcPdhgnYc=";
    }];
  }
]
