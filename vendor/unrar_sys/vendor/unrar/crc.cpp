// This CRC function is based on Intel Slicing-by-8 algorithm.
//
// Original Intel Slicing-by-8 code is available here:
//
//    http://sourceforge.net/projects/slicing-by-8/
//
// Original Intel Slicing-by-8 code is licensed as:
//    
//    Copyright (c) 2004-2006 Intel Corporation - All Rights Reserved
//    
//    This software program is licensed subject to the BSD License, 
//    available at http://www.opensource.org/licenses/bsd-license.html


#include "rar.hpp"

#ifndef SFX_MODULE
// User suggested to avoid BSD license in SFX module, so they do not need
// to include the license to SFX archive.
#define USE_SLICING
#endif

static uint crc_tables[16][256]; // Tables for Slicing-by-16.

#ifdef USE_NEON_CRC32
static bool CRC_Neon;
#endif


// Build the classic CRC32 lookup table.
// We also provide this function to legacy RAR and ZIP decryption code.
void InitCRC32(uint *CRCTab)
{
  if (CRCTab[1]!=0)
    return;
  for (uint I=0;I<256;I++)
  {
    uint C=I;
    for (uint J=0;J<8;J++)
      C=(C & 1) ? (C>>1)^0xEDB88320 : (C>>1);
    CRCTab[I]=C;
  }

#ifdef USE_NEON_CRC32
  #ifdef _APPLE
    // getauxval isn't available in OS X
    uint Value=0;
    size_t Size=sizeof(Value);
    int RetCode=sysctlbyname("hw.optional.armv8_crc32",&Value,&Size,NULL,0);
    CRC_Neon=RetCode==0 && Value!=0;
  #else
    CRC_Neon=(getauxval(AT_HWCAP) & HWCAP_CRC32)!=0;
  #endif
#endif

}


static void InitTables()
{
  InitCRC32(crc_tables[0]);

#ifdef USE_SLICING
  for (uint I=0;I<256;I++) // Build additional lookup tables.
  {
    uint C=crc_tables[0][I];
    for (uint J=1;J<16;J++)
    {
      C=crc_tables[0][(byte)C]^(C>>8);
      crc_tables[J][I]=C;
    }
  }
#endif
}


struct CallInitCRC {CallInitCRC() {InitTables();}} static CallInit32;

uint CRC32(uint StartCRC,const void *Addr,size_t Size)
{
  byte *Data=(byte *)Addr;

#ifdef USE_NEON_CRC32
  if (CRC_Neon)
  {
    for (;Size>=8;Size-=8,Data+=8)
#ifdef __clang__
      StartCRC = __builtin_arm_crc32d(StartCRC, RawGet8(Data));
#else
      StartCRC = __builtin_aarch64_crc32x(StartCRC, RawGet8(Data));
#endif
    for (;Size>0;Size--,Data++) // Process left data.
#ifdef __clang__
      StartCRC = __builtin_arm_crc32b(StartCRC, *Data);
#else
      StartCRC = __builtin_aarch64_crc32b(StartCRC, *Data);
#endif
    return StartCRC;
  }
#endif

#ifdef USE_SLICING
  // Align Data to 16 for better performance and to avoid ALLOW_MISALIGNED
  // check below.
  for (;Size>0 && ((size_t)Data & 15)!=0;Size--,Data++)
    StartCRC=crc_tables[0][(byte)(StartCRC^Data[0])]^(StartCRC>>8);

  // 2023.12.06: We switched to slicing-by-16, which seems to be faster than
  // slicing-by-8 on modern CPUs. Slicing-by-32 would require 32 KB for tables
  // and could be limited by L1 cache size on some CPUs.
  for (;Size>=16;Size-=16,Data+=16)
  {
#ifdef BIG_ENDIAN
    StartCRC ^= RawGet4(Data);
    uint D1 = RawGet4(Data+4);
    uint D2 = RawGet4(Data+8);
    uint D3 = RawGet4(Data+12);
#else
    // We avoid RawGet4 here for performance reason, to access uint32
    // directly even if ALLOW_MISALIGNED isn't defined. We can do it,
    // because we aligned 'Data' above.
    StartCRC ^= *(uint32 *) Data;
    uint D1 = *(uint32 *) (Data+4);
    uint D2 = *(uint32 *) (Data+8);
    uint D3 = *(uint32 *) (Data+12);
#endif
    StartCRC = crc_tables[15][(byte) StartCRC       ] ^
               crc_tables[14][(byte)(StartCRC >> 8) ] ^
               crc_tables[13][(byte)(StartCRC >> 16)] ^
               crc_tables[12][(byte)(StartCRC >> 24)] ^
               crc_tables[11][(byte) D1             ] ^
               crc_tables[10][(byte)(D1       >> 8) ] ^
               crc_tables[ 9][(byte)(D1       >> 16)] ^
               crc_tables[ 8][(byte)(D1       >> 24)] ^
               crc_tables[ 7][(byte) D2             ] ^
               crc_tables[ 6][(byte)(D2       >>  8)] ^
               crc_tables[ 5][(byte)(D2       >> 16)] ^
               crc_tables[ 4][(byte)(D2       >> 24)] ^
               crc_tables[ 3][(byte) D3             ] ^
               crc_tables[ 2][(byte)(D3       >>  8)] ^
               crc_tables[ 1][(byte)(D3       >> 16)] ^
               crc_tables[ 0][(byte)(D3       >> 24)];
  }
#endif

  for (;Size>0;Size--,Data++) // Process left data.
    StartCRC=crc_tables[0][(byte)(StartCRC^Data[0])]^(StartCRC>>8);

  return StartCRC;
}


#ifndef SFX_MODULE
// For RAR 1.4 archives in case somebody still has them.
ushort Checksum14(ushort StartCRC,const void *Addr,size_t Size)
{
  byte *Data=(byte *)Addr;
  for (size_t I=0;I<Size;I++)
  {
    StartCRC=(StartCRC+Data[I])&0xffff;
    StartCRC=((StartCRC<<1)|(StartCRC>>15))&0xffff;
  }
  return StartCRC;
}
#endif




