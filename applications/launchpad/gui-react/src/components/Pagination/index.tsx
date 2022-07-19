import ReactPaginate from 'react-paginate'
import { useTheme } from 'styled-components'

import Text from '../Text'

import SvgArrowLeft2 from '../../styles/Icons/ArrowLeft2'
import SvgArrowRight2 from '../../styles/Icons/ArrowRight2'

import t from '../../locales'

import {
  StyledPagination,
  PagesContainer,
  PaginationStatsContainer,
  SelectContainer,
} from './styles'
import { PaginationProps } from './types'
import Select from '../Select'
import { useMemo } from 'react'

/**
 * The pagination component
 * @param {number} currentPage - current active page
 * @param {number} perPage - records per page
 * @param {number} total - total number of records
 * @param {(val: number) => void} onPageChange - on page change
 */
const Pagination = ({
  currentPage,
  perPage,
  total,
  onPageChange,
}: PaginationProps) => {
  const theme = useTheme()

  const numberOfPages = Math.ceil(total / perPage)

  const options = useMemo(() => {
    return [...Array(numberOfPages).keys()].map(v => ({
      value: v,
      label: (v + 1).toString(),
      key: v.toString(),
    }))
  }, [perPage, total])

  const firstVisibleRecordNumber = currentPage * perPage + 1
  let lastVisibleRecordNumber = currentPage * perPage + perPage
  if (lastVisibleRecordNumber > total) {
    lastVisibleRecordNumber = total
  }

  return (
    <StyledPagination>
      <PagesContainer>
        <ReactPaginate
          forcePage={currentPage}
          pageCount={total ? numberOfPages : 1}
          onPageChange={({ selected }: { selected: number }) =>
            onPageChange(selected)
          }
          previousLabel={<SvgArrowLeft2 />}
          nextLabel={<SvgArrowRight2 />}
        />
      </PagesContainer>
      <PaginationStatsContainer>
        <Text color={theme.secondary}>
          {t.common.nouns.results}: {firstVisibleRecordNumber}-
          {lastVisibleRecordNumber} {t.common.conjunctions.of} {total}
        </Text>
        <SelectContainer>
          <Select
            options={options}
            value={{
              value: currentPage,
              label: (currentPage + 1).toString(),
              key: currentPage.toString(),
            }}
            onChange={opt => onPageChange(opt.value as number)}
            fullWidth
          />
        </SelectContainer>
      </PaginationStatsContainer>
    </StyledPagination>
  )
}

export default Pagination
